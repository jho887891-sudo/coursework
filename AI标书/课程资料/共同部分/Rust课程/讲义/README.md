# Rust 快速上手 — 为 Agent 开发做准备


---

## 你需要准备什么

| 要求 | 说明 |
|---|---|
| 编程基础 | 学过任意一门语言（Python/C/Java 都行） |
| Rust 已安装 | `rustc --version` ≥ 1.96 |
| 一个编辑器 | VSCode + rust-analyzer 插件 |

---

## 你会学到什么

几天密集学完 Agent 课程必需的基础。更深入的 Rust 知识（broadcast channel、Arc、子进程通信）在 Agent 课里遇到时穿插讲解。为项目研发做准备。

| 课次 | 内容 | 解决什么问题 |
|---|---|---|
| **第0课** | **环境搭建** | **cargo run 能跑通（开课前自学完成）** |
| 第1课 | 类型、结构体、枚举、所有权 | 能定义 ChatMessage、理解 String vs &str |
| 第2课 | 集合、迭代器 | 操作消息历史、搜索结果排序 |
| 第3课 | 错误处理、Trait | 用 ? 代替 unwrap、定义 Tool trait |
| 第4课 | 异步、HTTP 实战 | 调 DashScope API——Agent 第 1 课直接开始 |



---

## 和 Agent 课程的关系

```
PartA  Rust 前置（4 课）── 基本语法 + HTTP + JSON + async
PartB    Rust 大作业（ai-client crate）
PartC  Agent 课程 ── Rust 进阶知识在用到时穿插讲
PartD  Agent 大作业（Mini 标书审核）
```

Rust 进阶知识（trait object、broadcast channel、Arc、子进程 stdio）不放在前置课程里——脱离 Agent 场景讲这些太抽象。在 Agent 课里用到时，理解 Rust 概念。

---

## 学完你能做什么

- 定义 struct/enum 建模业务数据
- 用 Vec/HashMap 操作集合，用迭代器做数据转换
- 用 Result + ? + anyhow 写出不 panic 的代码
- 用 reqwest + serde + dotenv 调 HTTP API
- 用 tokio 写 async 函数并发请求
- 定义并实现 trait
