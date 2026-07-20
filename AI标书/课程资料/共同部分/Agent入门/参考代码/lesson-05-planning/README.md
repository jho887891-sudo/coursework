# Lesson 5 优秀学生示例：Plan → Execute → Verify → RePlan

本示例实现的不是“让模型输出步骤列表”，而是一套可验证、可执行、可停止的规划运行时。所有实验离线运行，不调用真实模型或网络服务。

## 已实现

- 兼容学生脚手架的 `Plan/Step`、稳定拓扑排序和 8 条原始验收；
- 富计划协议：objective、assumptions、typed action、依赖、成功标准、attempt 与成本上限；
- Validator：空目标、重复 ID、缺失依赖、未知工具、参数、成功标准、成本和依赖环；
- 只执行依赖全部成功的 Ready step，失败后继自动变成 Blocked；
- 独立 StepVerifier 与 GoalVerifier；
- 仅对 Transient 按 step budget 重试，Timeout/Permanent 不自动重试；
- 全局 Tool 调用硬预算；
- RePlan 次数上限、触发原因、计划谱系和兼容结果保留；
- 本地旅行 Tool 环境和幂等预订；
- Workflow、ReAct、Plan-Execute-RePlan 在同一30条任务上的指标比较；
- 全部30条公开旅行案例。

## 运行

在课程的 `示例代码` 目录执行：

```powershell
cargo test --offline -p lesson-05-planning --all-targets

cargo run --offline -p lesson-05-planning --example lesson5_demo -- graph
cargo run --offline -p lesson-05-planning --example lesson5_demo -- blocked
cargo run --offline -p lesson-05-planning --example lesson5_demo -- goal-failed
cargo run --offline -p lesson-05-planning --example lesson5_demo -- replan
cargo run --offline -p lesson-05-planning --example lesson5_demo -- compare
```

五个演示分别回答：

1. JSON 正确的计划为什么仍可能因依赖环而不可执行；
2. 一个永久失败如何阻止依赖它的步骤；
3. 为什么所有 Step 成功仍不等于整体目标完成；
4. RePlan 应保留什么、为什么不能无限重规划；
5. 三种控制架构如何在同一任务集和指标口径下比较。

## 推荐阅读顺序

1. `src/lib.rs` 中的最小 `Plan/Step` 与 `validate_and_sort`；
2. `ExecutablePlan/PlanStep/ToolCatalog` 富协议；
3. `execute_executable_plan` 的 Ready、retry、Blocked 和预算；
4. `StepVerifier/GoalVerifier` 的双层验证；
5. `ReplanController` 的结果保留与次数限制；
6. `TravelEnvironment` 与30条公开任务；
7. `tests/acceptance.rs`、`tests/safety_invariants.rs` 和 `tests/public_fixture_cases.rs`；
8. `REPORT.md` 的设计取舍。

