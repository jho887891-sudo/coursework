# Lesson 5 实验报告：受约束的规划、执行与重规划

## 1. 实验问题

本实验验证：计划由模型或其他 Planner 产生后，系统能否在执行前证明它合法，在执行中只运行 Ready step，在失败后阻断依赖链，并在所有步骤结束后独立判断用户目标是否真正满足。

核心不变量：

1. 非法计划在任何 Tool 调用前被拒绝；
2. 只有依赖全部 `Succeeded` 的步骤可以运行；
3. `Failed` 或 `Blocked` 的后继不会执行；
4. Step 成功必须经过 StepVerifier；
5. 所有 Step 成功仍必须经过 GoalVerifier；
6. 只有 `Transient` 可以在明确预算内重试；
7. RePlan 有原因、次数上限和计划谱系；
8. 仍有效的成功结果不会无条件重做；
9. 预订响应丢失不会产生第二次副作用；
10. 三种策略使用相同30条案例和相同指标定义。

## 2. 从“步骤列表”到可执行计划

富计划中的每一步包含：

```text
id
goal
typed action(tool + arguments)
depends_on
success_criteria
max_attempts
estimated_cost
```

Validator 的顺序是：

```text
计划 ID/目标
→ step ID 唯一
→ Tool 存在
→ 参数满足 Tool 契约
→ 成功标准非空
→ attempt 与成本预算
→ 依赖存在
→ 拓扑排序/环检测
```

计划能够反序列化只证明 JSON 合法，不证明存在 Ready step，也不证明它有权限、预算或完成判断。

## 3. 执行状态与失败传播

执行器不相信 Planner 给出的数组顺序，而使用稳定拓扑顺序：

```text
Pending → Ready → Running → Succeeded
                         └→ Failed
依赖 Failed/Blocked → Blocked
```

`Blocked` 不是一次 Tool 失败，而是“前置事实已经使该动作不可执行”。因此 Blocked step 的 attempt 必须为0。

## 4. 两层验证

StepVerifier 判断本步输出是否满足本步成功条件，例如输出必须包含 `price`、`total` 或 `status=reserved`。

GoalVerifier 判断整体约束，例如：

```text
total_cost <= user_budget
required reservation exists
```

测试专门构造了所有 Tool 都成功、总价却从2500上涨到2700的情况。最终状态必须是：

```text
all steps = Succeeded
goal = rejected
termination = goal_verifier_rejected
```

这证明“完成计划”与“完成目标”是两个不同命题。

## 5. 错误、重试与预算

| 错误类型 | 自动重试 |
|---|---:|
| Transient | step 的 max_attempts 和全局 Tool budget 都允许时 |
| Timeout | 否 |
| Permanent | 否 |
| PermissionDenied | 否 |

全局 Tool budget 在每次实际调用前检查。预算耗尽时不会为了“把计划跑完”多执行一次。

Timeout 不表示动作没有发生。旅行环境中的预订实验先提交副作用，再模拟响应丢失；第二次使用相同 idempotency key 时返回首次结果，预订计数仍为1。

## 6. RePlan 语义

每次重规划保存：

```text
old_plan_id
trigger
preserved_results
new_plan_id
```

只有 step ID 相同、action 完全兼容且旧结果存在时才保留结果。用户改变酒店日期后，旧交通结果可以保留，但旧酒店结果必须失效。

`max_replans` 是硬上限。达到上限后系统结束并报告，而不是继续请求 Planner 生成近似相同的计划。

## 7. 三种架构比较

30条旅行任务同时交给：

- 固定 Workflow；
- ReAct；
- Plan-Execute-RePlan。

统一记录：

- goal completion rate；
- constraint violation rate；
- average model calls；
- average tool calls；
- average latency；
- unnecessary replan rate。

比较结果不是用来预设 Planner 必胜。固定、稳定、无环境变化的任务通常更适合 Workflow；局部开放任务可以使用 ReAct；只有依赖图、多约束和环境变化同时存在时，Planning 的结构化成本才更可能值得。

## 8. 实验结果

- 原始8条课程验收全部解锁；
- 富协议和安全不变量测试覆盖 Validator、Blocked、Verifier、retry、预算、幂等和 RePlan；
- 30条公开旅行案例全部符合 expected；
- 非法计划进入 Tool 的次数为0；
- 两次瞬态失败在第三次恢复；
- Timeout 与永久错误只执行一次；
- 所有步骤成功但超预算时 GoalVerifier 拒绝；
- 响应丢失后的重复预订仍只产生一次副作用。

## 9. 已知边界

- 旅行 Tool 是确定性 fixture，不是真实供应商接口；
- 计划参数只做教学用字段存在性检查，生产系统应使用完整 JSON Schema；
- 执行器当前是确定性串行拓扑调度，未实现并行 Ready queue；
- 未实现 durable execution、持久化幂等和补偿事务；
- 延迟指标使用确定性教学成本模型，不能替代生产 profiling；
- Planner 在本课中是确定性策略，真实 LLM Planner 应接入 Lesson 2 的协议和预算。

## 10. 结论

Planning 的核心不是生成更多步骤，而是把计划当成需要验证的程序。Validator、拓扑执行、StepVerifier、GoalVerifier、预算、失败传播和有限 RePlan 共同决定计划是否真正可执行、可停止、可审计。
