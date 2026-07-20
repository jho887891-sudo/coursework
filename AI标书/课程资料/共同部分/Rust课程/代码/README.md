# Rust 作业示例


## 运行方式

在本目录（/代码）执行：

```powershell
cargo run -p lesson-01
cargo run -p lesson-02
cargo run -p lesson-03
cargo run -p lesson-04 -- ask "Rust 的所有权是什么"
cargo run -p lesson-04 -- translate "Hello, world" --to zh
cargo run -p ai-client --example demo //进入ai-client目录执行
cargo test --workspace
```

第 4 课和 Final Project 会真实请求 DashScope。先把对应目录中的 `.env.example` 复制为本目录的 `.env`，再填入自己的配置。不要提交真实 API Key !!!。

## 4个lesson + final_project

| 课程 | 内容 |
|------|--------|
| lesson-01 | 一个学生有姓名和多个成绩，程序里应该怎样表示？ |
| lesson-02 | 三篇法规文档，用户输入关键词找出匹配并按相关度排序|
| lesson-03 | 配置文件写错了怎么办？如何进行优雅的错误处理。 |
| lesson-04 | 写一个命令行 AI 助手，支持问答和翻译 |
| ai-client | 把 call_llm 改造成别人可以 import 的库|
