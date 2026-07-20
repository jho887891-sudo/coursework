# Agent 入门课程：逐 Lesson 实现脚手架

这里不是参考答案。每个 Lesson 是独立 Rust crate，只提供：

- 能编译的类型与接口；
- 明确的 `NotImplemented` 占位行为；
- 默认通过的 smoke test；
- 默认标记为 `ignored` 的分层验收测试。

学生应在对应 Lesson 中实现核心机制，再运行该 Lesson 的验收测试。独立 crate 可以让初学者把注意力集中在当前问题；Final Project 再重新组合这些能力。

```powershell
# 先确认所有脚手架可编译
cargo test --workspace

# 以 Lesson 1 为例：运行尚未解锁的验收测试
cargo test -p lesson-01-agent-model --test acceptance -- --ignored
```

验收测试初始失败是正常现象。不要删除断言或把错误改成默认成功；实现接口，使测试从失败变为通过。每个 crate 的 `README.md` 说明实现顺序。

