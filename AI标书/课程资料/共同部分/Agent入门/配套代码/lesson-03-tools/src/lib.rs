use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    UnknownTool,
    InvalidArgs,
    PermissionDenied,
    PathOutsideWorkspace,
    Timeout,
    Transient,
    Permanent,
    DuplicateSideEffect,
}
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: &'static str,
    pub has_side_effect: bool,
    pub max_input_len: usize,
}
#[derive(Debug, Clone)]
pub struct ToolRequest {
    pub name: String,
    pub input: String,
    pub idempotency_key: Option<String>,
}
#[derive(Debug, Clone)]
pub struct ExecutionPolicy {
    pub allow_side_effect: bool,
    pub max_retries: usize,
}
pub trait Tool {
    fn spec(&self) -> ToolSpec;
    fn call(&mut self, input: &str) -> Result<String, ToolError>;
}
#[derive(Default)]
pub struct Registry {
    tools: HashMap<String, Box<dyn Tool>>,
    completed: HashMap<String, String>,
}
impl Registry {
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.spec().name.into(), tool);
    }
    pub fn execute(
        &mut self,
        _request: &ToolRequest,
        _policy: &ExecutionPolicy,
    ) -> Result<String, ToolError> {
        Err(ToolError::UnknownTool)
    }
    pub fn completed_count(&self) -> usize {
        self.completed.len()
    }
}
pub fn authorize_path(_workspace_root: &Path, _requested: &Path) -> Result<PathBuf, ToolError> {
    Err(ToolError::PathOutsideWorkspace)
}
#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_compiles() {
        assert!(true);
    }
}
