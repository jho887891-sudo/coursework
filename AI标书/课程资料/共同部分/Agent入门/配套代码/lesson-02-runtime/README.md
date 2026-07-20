# Lesson 2：实现可靠 Runtime

实现动作解析、model/step/tool/time 四类预算、确定性终止、重复检测和 Trace。不得依赖真实模型完成测试。

```powershell
cargo test -p lesson-02-runtime --test acceptance -- --ignored
```

