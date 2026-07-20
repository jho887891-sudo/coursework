# 文档解析实战：从 PDF 内核到生产级解析 Pipeline

> 5 天深度原理课。文档解析引擎组主修。Rust 为主、Python 用于 ML 推理，通过独立 Demo 理解 PDF 结构、子进程协议、多引擎编排、Schema 和 bbox。

本课遵循[专项课程统一学习规范](../../专项课程统一学习规范.md)。实验使用自备小样本和独立工程，不修改现有项目解析代码。

---

## 你需要准备什么

| 要求 | 说明 |
|------|------|
| Rust | struct/enum/trait/async/Result/reqwest/serde/tokio |
| Python 3.10+ | `pip install docling mineru paddleocr` |
| PDF 零基础可学 | Day 1 从 hexdump 讲起 |
| Docker | Redis + MinIO |

### 验证环境

```bash
rustc --version       # ≥ 1.96
python --version      # ≥ 3.10
python -c "import docling; print('docling OK')"
python -c "import paddleocr; print('paddleocr OK')"
```

---

## 你会学到什么

| 天次 | 主题 | 你会理解或验证什么 |
|------|------|------------|
| Day 1 | **PDF 内核 + Rust 绑定** | Rust PDF 结构解析器 + 文本提取器 + 中文乱码诊断 |
| Day 2 | **Rust 驾驭 Python ML** | 子进程 JSON 协议 + Docling 调用器 + 超时降级管理器 |
| Day 3 | **多引擎编排 + Schema** | 三引擎调度器 + 结果融合 + ParsedDocument serde |
| Day 4 | **质量评估 + bbox 链路** | 确定性 Block ID + 坐标变换 + 表格识别 Benchmark |
| Day 5 | **Pipeline 机制实验** | 用 Mock 队列和本地文件模拟去重、分片、失败恢复与回调 |

---

## 代码怎么写

**Rust 为主，Python 负责推理 Demo。** 下列结构是完整系统的理解模型，不要求每位成员一次性全部实现。

```
Rust (主进程)：
├── pdf_reader.rs       ← Day 1：解析 PDF 二进制结构
├── python_bridge.rs    ← Day 2：std::process::Command + JSON 协议
├── orchestrator.rs     ← Day 3：三引擎调度 + 融合
├── schema.rs           ← Day 4：ParsedDocument + Block ID
├── worker.rs           ← Day 5：Redis Streams 消费 + 分片
└── main.rs             ← 入口：axum HTTP API

Python (ML 推理容器)：
├── docling_worker.py   ← 接收 JSON→调 Docling→输出 JSON
├── mineru_worker.py    ← 接收 JSON→调 MinerU→输出 JSON
└── ocr_worker.py       ← 接收 JSON→调 PaddleOCR→输出 JSON
```

---

## 与标书审核项目的关系

```
本课程 → G1 文档解析组
  ├─ Day 3 ParsedDocument → 讨论项目需要的解析接口契约
  ├─ Day 4 Block ID + bbox → G9 前端高亮定位的数据源头
  └─ Day 5 Pipeline → 对照现有 pdf_extract/chunking 服务理解生产链路

下游消费：
  G2 规则引擎 ← ParsedDocument JSON
  G3 知识检索 ← Chunk 文本 + bbox_refs
  G4 Agent   ← clause_id + source_quote
  G9 前端    ← bbox 坐标 + 页面号
```
