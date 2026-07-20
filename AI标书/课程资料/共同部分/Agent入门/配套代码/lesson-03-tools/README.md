# Lesson 3：实现 Tool Registry 与失败恢复

实现注册、查找、参数校验、权限、错误分类和有限重试。重点测试未知工具、路径越界和非幂等副作用。

```powershell
cargo test -p lesson-03-tools --test acceptance -- --ignored
```

