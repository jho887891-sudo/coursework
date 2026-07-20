# Day 2：Rust 驾驭 Python ML — 子进程协议 + 进程池

> Day 1 你理解了 PDF 内核。但生产环境解析 200 页招标文件不可能用你的 Rust 手写提取器——你需要 Docling 的版面分析和表格识别。Docling 是 Python 写的。今天你用 Rust 驾驭它——子进程 JSON 协议、进程池复用、超时降级。

---

## 学习目标

1. 设计 Rust↔Python 的 stdin/stdout JSON 单行协议
2. 用 `std::process::Command` 调用 Python ML 脚本并管理生命周期
3. 实现进程池（pre-fork）+ 超时/重试/降级
4. 用 `serde` 校验 Python 输出的 JSON Schema

---

## 核心概念

### 1. Rust↔Python JSON 协议

**为什么不用 PyO3**：PyO3 需要 CPython 嵌入——GIL 锁、内存管理、版本绑定、调试困难。子进程方案：Rust 和 Python 完全独立，Python 升级/崩溃不影响 Rust 主进程。

**协议设计**：一行 JSON = 一次请求/响应。

```
Rust → Python (stdin):
{"action":"parse","file_path":"/data/bid.pdf","options":{"extract_tables":true}}
\n

Python → Rust (stdout):
{"status":"ok","result":{"pages":[{"page_num":1,"blocks":[...],"tables":[...]}]}}
\n
```

**为什么是一行 JSON 而不是多行**：`BufReader::read_line` 读到 `\n` 就是一条完整响应。不需要自定义帧协议。`serde_json::from_str` 直接反序列化。

```rust
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio, Child, ChildStdin};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct ParseRequest {
    action: String,
    file_path: String,
    options: ParseOptions,
}

#[derive(Deserialize)]
struct ParseResponse {
    status: String,       // "ok" | "error"
    result: Option<serde_json::Value>,
    message: Option<String>,
}

struct PythonWorker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

impl PythonWorker {
    fn spawn(script: &str) -> Result<Self> {
        let mut child = Command::new("python")
            .arg(script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());

        Ok(PythonWorker { child, stdin, stdout })
    }

    fn parse(&mut self, file_path: &str) -> Result<serde_json::Value> {
        let request = ParseRequest {
            action: "parse".into(),
            file_path: file_path.into(),
            options: ParseOptions { extract_tables: true },
        };

        // 发送请求
        serde_json::to_writer(&mut self.stdin, &request)?;
        writeln!(self.stdin)?;  // \n 结束
        self.stdin.flush()?;

        // 读取响应
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let response: ParseResponse = serde_json::from_str(&line)?;

        match response.status.as_str() {
            "ok" => Ok(response.result.unwrap()),
            "error" => Err(anyhow::anyhow!("Python: {}", response.message.unwrap_or_default())),
            _ => Err(anyhow::anyhow!("Unknown status: {}", response.status)),
        }
    }
}
```

### 2. Python 端实现

```python
# docling_worker.py
import sys
import json
from docling.document_converter import DocumentConverter

converter = DocumentConverter()  # 进程启动时加载模型一次

def handle_parse(file_path: str, options: dict) -> dict:
    result = converter.convert(file_path)
    doc = result.document
    
    pages = []
    for page_num, page in enumerate(doc.pages, 1):
        blocks = []
        for item in page.items:
            blocks.append({
                "type": item.label,
                "text": item.text,
                "bbox": {"x": item.bbox[0], "y": item.bbox[1],
                         "w": item.bbox[2]-item.bbox[0], "h": item.bbox[3]-item.bbox[1]},
            })
        tables = []  # Docling 的表格提取
        for table in page.tables:
            tables.append({
                "caption": table.caption,
                "rows": [[cell.text for cell in row] for row in table.rows],
                "bbox": {"x": table.bbox[0], ...},
            })
        pages.append({"page_num": page_num, "blocks": blocks, "tables": tables})
    
    return {"pages": pages}

# 主循环：读一行→处理→写一行
for line in sys.stdin:
    req = json.loads(line.strip())
    if req["action"] == "parse":
        try:
            result = handle_parse(req["file_path"], req.get("options", {}))
            resp = {"status": "ok", "result": result}
        except Exception as e:
            resp = {"status": "error", "message": str(e)}
        print(json.dumps(resp), flush=True)
```

### 3. 进程池 — 为什么不能每次启动新进程

Docling 的 `DocumentConverter` 初始化需要加载 ML 模型到内存——3-5 秒。如果你对每份 PDF 启动一个新 Python 进程→前 3-5 秒在加载模型，后 2 秒在做解析→70% 时间在加载。

**进程池**：预启动 4 个 Python worker 进程（每个预加载模型），Rust 端轮询分配。Worker 空闲→分配任务。Worker 全部忙→排队等待。

```rust
use std::sync::{Arc, Mutex};

struct ProcessPool {
    workers: Arc<Mutex<Vec<PythonWorker>>>,
    size: usize,
}

impl ProcessPool {
    fn new(script: &str, size: usize) -> Result<Self> {
        let workers = (0..size)
            .map(|_| PythonWorker::spawn(script))
            .collect::<Result<Vec<_>>>()?;
        Ok(ProcessPool { workers: Arc::new(Mutex::new(workers)), size })
    }

    async fn parse(&self, file_path: &str) -> Result<serde_json::Value> {
        // 获取空闲 worker
        let mut worker = loop {
            if let Some(w) = self.workers.lock().unwrap().pop() {
                break w;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        };

        // 执行（阻塞 IO → spawn_blocking）
        let file_path = file_path.to_string();
        let result = tokio::task::spawn_blocking(move || {
            let r = worker.parse(&file_path);
            (worker, r)  // 返回 worker 以便回收
        }).await?;

        // 回收 worker 到池中
        self.workers.lock().unwrap().push(result.0);
        result.1
    }
}
```

### 4. 超时与降级

```rust
pub async fn parse_with_fallback(pool: &ProcessPool, file_path: &str) -> ParsedDocument {
    match tokio::time::timeout(Duration::from_secs(30), pool.parse(file_path)).await {
        Ok(Ok(result)) => ParsedDocument::from_docling(result),
        Ok(Err(e)) => {
            log::warn!("Docling failed: {e}, falling back to pdfplumber");
            fallback_pdfplumber(file_path)  // 纯文本提取，精度低但不阻塞
        }
        Err(_) => {
            log::error!("Docling timeout after 30s, falling back to minimal parse");
            minimal_parse(file_path)  // 只提取文件名+文件大小+页码数
        }
    }
}
```

---

## 动手

### 任务 1：Rust↔Python 通信

写一个简单的 Python 脚本（`echo_worker.py`）——读 stdin 一行 JSON，原样写回 stdout。Rust 端实现 `PythonWorker` struct→验证可以双向通信。

### 任务 2：Docling 调用器

用 `docling_worker.py` 替换 echo_worker。Rust 端调用→解析招标文件 PDF→拿到 JSON 结果→`serde` 反序列化→提取页面数/block 数/表格数。

### 任务 3：进程池 + 容错

实现 4 worker 进程池→同时解析 10 份 PDF（4 个并发+6 个排队）→验证池工作正常。模拟 Python 进程超时（在 worker 中加 `sleep(40)`）→验证超时降级到 pdfplumber。

---

## 验收标准

- [ ] `PythonWorker` 正确发送/接收 JSON
- [ ] Docling 解析结果正确提取页面/block/表格
- [ ] 进程池 10 并发——4 并发执行 + 6 排队——全部完成
- [ ] 超时降级：Python worker 超时→自动回退

---

## 思考题

1. `std::process::Command` 创建的子进程——如果 Rust 主进程 panic，Python 子进程会被 kill 吗？（提示：`Drop` trait + `Child` 的 `kill()`）
2. 进程池的 `Arc<Mutex<Vec<PythonWorker>>>` 在高并发下——锁竞争会成瓶颈吗？怎么用 mpsc channel 替代锁？
3. Python 端输出了 100MB 的 JSON——`BufReader::read_line` 读到 `\n` 才停。这一行可能有多大？会不会 OOM？

---

## 与标书审核项目的关系

独立 Demo 使用 Rust 主进程通过 `std::process::Command` 调用 Python 脚本，以理解进程协议、超时和错误隔离。完成后只读对照项目解析服务的相关职责；Demo 的 `ProcessPool` 和 `docling_worker.py` 不直接写入项目。
