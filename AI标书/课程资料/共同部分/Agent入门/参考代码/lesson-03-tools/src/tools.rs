use crate::{
    FieldSpec, Permission, Schema, SideEffect, Tool, ToolContext, ToolError, ToolErrorKind,
    ToolSpec,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};
use tokio::time::{sleep, Duration};

fn object_output(fields: Vec<FieldSpec>) -> Schema {
    Schema::object(fields)
}

fn base_spec(
    name: &str,
    description: &str,
    input_schema: Schema,
    output_schema: Schema,
) -> ToolSpec {
    ToolSpec {
        name: name.to_owned(),
        description: description.to_owned(),
        input_schema,
        output_schema,
        side_effect: SideEffect::None,
        required_permissions: Vec::new(),
        requires_idempotency_key: false,
        timeout_ms: 100,
        max_input_bytes: 4 * 1024,
        max_output_bytes: 64 * 1024,
    }
}

#[derive(Debug, Default)]
pub struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn spec(&self) -> ToolSpec {
        base_spec(
            "echo",
            "原样返回非空文本；用于验证 Tool Registry 数据流",
            Schema::object(vec![FieldSpec::required(
                "text",
                Schema::non_empty_string(8),
            )]),
            object_output(vec![FieldSpec::required(
                "text",
                Schema::non_empty_string(8),
            )]),
        )
    }

    async fn call(&mut self, arguments: Value, _context: &ToolContext) -> Result<Value, ToolError> {
        Ok(json!({"text": arguments["text"]}))
    }
}

pub fn authorize_path(workspace_root: &Path, requested: &Path) -> Result<PathBuf, ToolError> {
    let root = workspace_root.canonicalize().map_err(|error| {
        ToolError::new(
            ToolErrorKind::Io,
            format!("workspace root 无法解析：{error}"),
        )
    })?;
    let candidate = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        root.join(requested)
    };
    let normalized = normalize_lexically(&candidate);
    if !normalized.starts_with(&root) {
        return Err(ToolError::new(
            ToolErrorKind::PathOutsideWorkspace,
            format!("路径越过 workspace：{}", requested.display()),
        ));
    }

    if normalized.exists() {
        let canonical = normalized.canonicalize().map_err(|error| {
            ToolError::new(
                ToolErrorKind::Io,
                format!("路径无法解析：{}：{error}", requested.display()),
            )
        })?;
        if !canonical.starts_with(&root) {
            return Err(ToolError::new(
                ToolErrorKind::PathOutsideWorkspace,
                "符号链接或规范化路径指向 workspace 外部",
            ));
        }
        return Ok(canonical);
    }
    Ok(normalized)
}

fn normalize_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

#[derive(Debug, Clone)]
pub struct ReadWorkspaceFile {
    root: PathBuf,
    max_file_bytes: usize,
}

impl ReadWorkspaceFile {
    pub fn new(root: impl AsRef<Path>, max_file_bytes: usize) -> Result<Self, ToolError> {
        let root = root.as_ref().canonicalize().map_err(|error| {
            ToolError::new(
                ToolErrorKind::Io,
                format!("workspace root 无法读取：{error}"),
            )
        })?;
        Ok(Self {
            root,
            max_file_bytes,
        })
    }
}

#[async_trait]
impl Tool for ReadWorkspaceFile {
    fn spec(&self) -> ToolSpec {
        let mut spec = base_spec(
            "read_fixture",
            "只读取授权 workspace 内的小型文本文件",
            Schema::object(vec![FieldSpec::required(
                "path",
                Schema::non_empty_string(512),
            )]),
            object_output(vec![
                FieldSpec::required("path", Schema::non_empty_string(512)),
                FieldSpec::required(
                    "content",
                    Schema::String {
                        min_len: 0,
                        max_len: self.max_file_bytes,
                    },
                ),
            ]),
        );
        spec.required_permissions = vec![Permission::ReadWorkspace];
        spec.max_output_bytes = self.max_file_bytes.saturating_add(1024);
        spec
    }

    async fn call(&mut self, arguments: Value, _context: &ToolContext) -> Result<Value, ToolError> {
        let requested = Path::new(
            arguments["path"]
                .as_str()
                .expect("Registry validates path as string"),
        );
        let authorized = authorize_path(&self.root, requested)?;
        let allowed_extension = authorized
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| matches!(extension, "txt" | "md" | "json" | "jsonl"));
        if !allowed_extension {
            return Err(ToolError::new(
                ToolErrorKind::PermissionDenied,
                "只允许读取 txt、md、json、jsonl 文件",
            ));
        }
        let metadata = fs::metadata(&authorized).map_err(|error| {
            ToolError::new(ToolErrorKind::Io, format!("无法读取文件元数据：{error}"))
        })?;
        if metadata.len() > self.max_file_bytes as u64 {
            return Err(ToolError::new(
                ToolErrorKind::OutputTooLarge,
                format!(
                    "文件大小 {} bytes 超过限制 {} bytes",
                    metadata.len(),
                    self.max_file_bytes
                ),
            ));
        }
        let content = fs::read_to_string(&authorized)
            .map_err(|error| ToolError::new(ToolErrorKind::Io, error.to_string()))?;
        let relative = authorized
            .strip_prefix(&self.root)
            .unwrap_or(&authorized)
            .to_string_lossy()
            .replace('\\', "/");
        Ok(json!({"path": relative, "content": content}))
    }
}

#[derive(Debug, Clone)]
pub struct SearchText {
    root: PathBuf,
    max_matches: usize,
}

impl SearchText {
    pub fn new(root: impl AsRef<Path>, max_matches: usize) -> Result<Self, ToolError> {
        let root = root.as_ref().canonicalize().map_err(|error| {
            ToolError::new(ToolErrorKind::Io, format!("search root 无法读取：{error}"))
        })?;
        Ok(Self { root, max_matches })
    }
}

#[async_trait]
impl Tool for SearchText {
    fn spec(&self) -> ToolSpec {
        let match_schema = Schema::object(vec![
            FieldSpec::required("path", Schema::non_empty_string(512)),
            FieldSpec::required("line", Schema::Integer),
            FieldSpec::required("excerpt", Schema::non_empty_string(2048)),
        ]);
        let mut spec = base_spec(
            "search_text",
            "在授权 workspace 的文本文件中进行大小写敏感搜索",
            Schema::object(vec![FieldSpec::required(
                "query",
                Schema::non_empty_string(128),
            )]),
            object_output(vec![FieldSpec::required(
                "matches",
                Schema::array(match_schema, self.max_matches),
            )]),
        );
        spec.required_permissions = vec![Permission::ReadWorkspace];
        spec
    }

    async fn call(&mut self, arguments: Value, _context: &ToolContext) -> Result<Value, ToolError> {
        let query = arguments["query"]
            .as_str()
            .expect("Registry validates query as string");
        let mut files = Vec::new();
        collect_text_files(&self.root, &mut files)?;
        files.sort();
        let mut matches = Vec::new();
        'files: for file in files {
            let authorized = authorize_path(&self.root, &file)?;
            let content = fs::read_to_string(&authorized)
                .map_err(|error| ToolError::new(ToolErrorKind::Io, error.to_string()))?;
            for (index, line) in content.lines().enumerate() {
                if line.contains(query) {
                    matches.push(json!({
                        "path": authorized.strip_prefix(&self.root)
                            .unwrap_or(&authorized)
                            .to_string_lossy()
                            .replace('\\', "/"),
                        "line": (index + 1) as i64,
                        "excerpt": line
                    }));
                    if matches.len() >= self.max_matches {
                        break 'files;
                    }
                }
            }
        }
        Ok(json!({"matches": matches}))
    }
}

fn collect_text_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), ToolError> {
    for entry in fs::read_dir(directory)
        .map_err(|error| ToolError::new(ToolErrorKind::Io, error.to_string()))?
    {
        let entry = entry.map_err(|error| ToolError::new(ToolErrorKind::Io, error.to_string()))?;
        let file_type = entry
            .file_type()
            .map_err(|error| ToolError::new(ToolErrorKind::Io, error.to_string()))?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_text_files(&entry.path(), files)?;
        } else if entry
            .path()
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| matches!(extension, "txt" | "md"))
        {
            files.push(entry.path());
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
struct CounterState {
    counts: HashMap<String, i64>,
    completed: HashMap<(String, String), Value>,
    lose_response_once: bool,
}

#[derive(Debug, Clone)]
pub struct CounterHandle {
    state: Arc<Mutex<CounterState>>,
}

impl CounterHandle {
    pub fn value(&self, key: &str) -> i64 {
        self.state
            .lock()
            .expect("counter mutex poisoned")
            .counts
            .get(key)
            .copied()
            .unwrap_or(0)
    }
}

#[derive(Debug)]
pub struct CounterTool {
    state: Arc<Mutex<CounterState>>,
    timeout_ms: u64,
}

impl CounterTool {
    pub fn new(timeout_ms: u64) -> Self {
        Self {
            state: Arc::new(Mutex::new(CounterState::default())),
            timeout_ms,
        }
    }

    pub fn with_lost_response_once(timeout_ms: u64) -> Self {
        let tool = Self::new(timeout_ms);
        tool.state
            .lock()
            .expect("counter mutex poisoned")
            .lose_response_once = true;
        tool
    }

    pub fn handle(&self) -> CounterHandle {
        CounterHandle {
            state: Arc::clone(&self.state),
        }
    }
}

#[async_trait]
impl Tool for CounterTool {
    fn spec(&self) -> ToolSpec {
        let mut spec = base_spec(
            "counter_add",
            "为指定计数器增加整数；必须使用稳定的 idempotency key",
            Schema::object(vec![
                FieldSpec::required("key", Schema::non_empty_string(64)),
                FieldSpec::required("amount", Schema::Integer),
            ]),
            object_output(vec![
                FieldSpec::required("key", Schema::non_empty_string(64)),
                FieldSpec::required("value", Schema::Integer),
            ]),
        );
        spec.side_effect = SideEffect::Reversible;
        spec.required_permissions = vec![Permission::ModifyState];
        spec.requires_idempotency_key = true;
        spec.timeout_ms = self.timeout_ms;
        spec
    }

    async fn call(&mut self, arguments: Value, context: &ToolContext) -> Result<Value, ToolError> {
        let key = arguments["key"]
            .as_str()
            .expect("Registry validates counter key")
            .to_owned();
        let amount = arguments["amount"]
            .as_i64()
            .expect("Registry validates amount");
        let idempotency_key = context
            .idempotency_key
            .as_ref()
            .expect("Registry requires idempotency key")
            .clone();

        let (output, lose_response) = {
            let mut state = self.state.lock().expect("counter mutex poisoned");
            if let Some(output) = state
                .completed
                .get(&(key.clone(), idempotency_key.clone()))
                .cloned()
            {
                return Ok(output);
            }
            let value = {
                let current = state.counts.entry(key.clone()).or_insert(0);
                *current += amount;
                *current
            };
            let output = json!({"key": key, "value": value});
            state
                .completed
                .insert((key, idempotency_key), output.clone());
            let lose_response = state.lose_response_once;
            state.lose_response_once = false;
            (output, lose_response)
        };

        if lose_response {
            sleep(Duration::from_millis(self.timeout_ms.saturating_add(50))).await;
        }
        Ok(output)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultMode {
    Success,
    Transient,
    Permanent,
    Timeout,
    MalformedOutput,
    PermissionDenied,
}

#[derive(Debug)]
pub struct FaultyTool {
    script: VecDeque<FaultMode>,
    timeout_ms: u64,
    calls: Arc<AtomicUsize>,
}

impl FaultyTool {
    pub fn new(script: impl IntoIterator<Item = FaultMode>, timeout_ms: u64) -> Self {
        Self {
            script: script.into_iter().collect(),
            timeout_ms,
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn calls_handle(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.calls)
    }
}

#[async_trait]
impl Tool for FaultyTool {
    fn spec(&self) -> ToolSpec {
        let mut spec = base_spec(
            "faulty",
            "按脚本产生确定性成功、瞬态、永久、超时或格式错误",
            Schema::object(Vec::new()),
            object_output(vec![FieldSpec::required(
                "status",
                Schema::non_empty_string(16),
            )]),
        );
        spec.timeout_ms = self.timeout_ms;
        spec
    }

    async fn call(
        &mut self,
        _arguments: Value,
        _context: &ToolContext,
    ) -> Result<Value, ToolError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.script.pop_front().unwrap_or(FaultMode::Permanent) {
            FaultMode::Success => Ok(json!({"status":"ok"})),
            FaultMode::Transient => Err(ToolError::new(ToolErrorKind::Transient, "模拟瞬态错误")),
            FaultMode::Permanent => Err(ToolError::new(ToolErrorKind::Permanent, "模拟永久错误")),
            FaultMode::Timeout => {
                sleep(Duration::from_millis(self.timeout_ms.saturating_add(50))).await;
                Ok(json!({"status":"late"}))
            }
            FaultMode::MalformedOutput => Ok(json!({"unexpected":true})),
            FaultMode::PermissionDenied => Err(ToolError::new(
                ToolErrorKind::PermissionDenied,
                "工具内部权限拒绝",
            )),
        }
    }
}
