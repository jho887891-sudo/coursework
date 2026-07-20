# 第5课：Planning — 从固定 Workflow 到 RePlan

> “让模型输出一个步骤列表”只是计划生成。规划系统还必须验证计划、执行动作、观察状态变化，并在假设失效时重新规划。

---

## 学习目标

1. 区分 Workflow、ReAct 和 Plan-Execute-RePlan；
2. 用前置条件、依赖和成功标准描述计划；
3. 检测循环依赖、未知工具和不可执行步骤；
4. 在环境变化或步骤失败后 RePlan；
5. 用完成率、成本和延迟比较策略。

---

## 本 Lesson 必须实现

工程：`配套代码/lesson-05-planning`，数据：`公开数据/lesson-05-travel/`。

1. 完成 Plan/Step schema 与 `validate_and_sort`；
2. 检查重复 ID、缺失依赖、未知工具、空目标和依赖环；
3. 按拓扑顺序执行 Ready Step，并传播 Failed/Blocked；
4. 独立实现 StepVerifier 与 `goal_satisfied`；
5. 定义 RePlan trigger、保留结果和最大次数。

```powershell
cargo test -p lesson-05-planning --test acceptance -- --ignored
```

先实现 Validator 和确定性执行器，再接模型生成计划。计划能解析、步骤都成功，仍不等于目标完成。

---

## 1. 三种控制方式

### 固定 Workflow

```text
parse → retrieve → analyze → report
```

稳定、便宜、容易测试。任务结构固定时优先考虑。

### ReAct

```text
观察 → 选择一个动作 → 获得新观察 → 再选择
```

灵活，但容易局部游走、重复动作或忘记全局目标。

### Plan-Execute-RePlan

```text
建立计划 → 校验 → 执行可运行步骤 → 检查目标
              ↑                    ↓
              └──── 假设失效时重规划
```

适合多步骤、依赖明确、执行中可能出现新信息的任务。

---

## 2. 可执行计划

```rust
struct PlanStep {
    id: String,
    goal: String,
    action: PlannedAction,
    depends_on: Vec<String>,
    success_criteria: Vec<String>,
    max_attempts: u8,
}

struct Plan {
    objective: String,
    assumptions: Vec<String>,
    steps: Vec<PlanStep>,
}
```

一份有效计划至少满足：

- step id 唯一；
- 所有依赖存在；
- 依赖图无环；
- action 在 Tool Registry 中；
- 参数通过校验；
- 每一步有完成判断；
- 总成本不超过预算。

`serde(default)` 不能把缺少关键字段的计划自动变成正确计划。

---

## 3. 计划执行器

执行器不应相信模型给出的步骤顺序，而应根据依赖图选择 ready steps：

```text
Pending → Ready → Running → Succeeded
                         └→ Failed
Pending + dependency failed → Blocked
```

必须区分：

- 步骤执行成功；
- 步骤输出满足成功标准；
- 整体目标已经完成。

工具返回 200 或 `Ok`，不代表任务目标完成。

---

## 4. 什么时候 RePlan

常见触发条件：

- 计划依赖的文件不存在；
- 工具永久失败；
- 新观察推翻了计划假设；
- 用户改变目标；
- 剩余预算不足；
- 所有步骤完成但目标验证失败。

RePlan 不能无限进行。需要 `max_replans`，并保留旧计划、变更理由和新计划之间的关系。

---

## 5. Worked Example：一个计划为何“JSON 正确但不可执行”

模型输出：

```json
{
  "objective":"制定预算内的两日行程",
  "steps":[
    {"id":"s1","goal":"选择酒店","action":"search_hotel","depends_on":["s2"]},
    {"id":"s2","goal":"计算总价","action":"calculate_cost","depends_on":["s1"]}
  ]
}
```

JSON 可以解析，但 `s1 → s2 → s1` 形成环，没有任何 Ready step。

### 依赖校验思路

可以使用拓扑排序：

```text
1. 计算每个节点的入度
2. 把入度为 0 的节点加入队列
3. 依次移除节点并减少后继入度
4. 最终移除数量小于总节点数 → 存在环
```

简化伪代码：

```rust
fn validate_acyclic(plan: &Plan) -> Result<(), PlanError> {
    let mut indegree = build_indegree(plan)?;
    let mut ready = collect_zero_indegree(&indegree);
    let mut visited = 0;

    while let Some(step) = ready.pop_front() {
        visited += 1;
        for next in dependents_of(&step) {
            let value = indegree
                .get_mut(&next)
                .ok_or_else(|| PlanError::UnknownDependency(next.clone()))?;
            *value -= 1;
            if *value == 0 {
                ready.push_back(next.clone());
            }
        }
    }

    if visited != plan.steps.len() {
        return Err(PlanError::CyclicDependency);
    }
    Ok(())
}
```

---

## 6. Worked Example：执行成功但目标失败

计划：

```text
s1 search_transport → 成功，价格 1800
s2 search_hotel     → 成功，价格 1600
s3 calculate_cost   → 成功，总价 3400
```

用户预算是 3000。所有 Tool 都返回成功，但 Goal Verifier 应判：

```text
constraint_violation: total_cost > budget
```

然后选择：

- RePlan，搜索更便宜选项；
- 请求用户调整预算；
- 在预算不足时结束并说明原因。

不能因为每个 step 都是 `Succeeded` 就输出“任务完成”。

---

## 7. Plan Executor 分步实现

### Checkpoint A：数据结构

建立 `Plan`、`PlanStep`、`StepStatus`、`StepResult`。先用手写计划，不调用模型。

### Checkpoint B：Validator

按顺序检查：

1. id 唯一；
2. 依赖存在；
3. 无环；
4. action 已注册；
5. 参数合法；
6. success criteria 非空；
7. 估算成本未超预算。

每类错误使用不同 `PlanError`。

### Checkpoint C：Ready Queue

只有依赖全部 `Succeeded` 的 step 才能进入 Ready。依赖 `Failed` 的 step 标记 `Blocked`。

### Checkpoint D：Goal Verifier

先用规则验证预算、日期和必需产物。不要一开始把所有验证交给 LLM。

### Checkpoint E：RePlan

保存：

```rust
struct ReplanRecord {
    old_plan_id: String,
    trigger: String,
    preserved_results: Vec<String>,
    new_plan_id: String,
}
```

已经成功且仍有效的结果不应无条件重做。

---

## 8. 如何公平比较三种策略

控制以下变量一致：

- 相同模型；
- 相同工具和 fixture；
- 相同总调用预算；
- 相同任务集；
- 相同成功条件；
- 相同超时设置。

否则“Planner 成功率更高”可能只是因为它获得了更多调用次数。

---

## 9. 常见错误排查

| 现象 | 检查 |
|---|---|
| 没有 Ready step | 依赖环或所有依赖未完成 |
| Failed step 的后继仍执行 | Blocked 传播 |
| 一直 RePlan | trigger 是否重复、max_replans |
| 新计划重复旧错误 | 是否把失败 Observation 提供给 Planner |
| 所有步骤成功但结果不合格 | Goal Verifier 与硬约束 |

---

## 10. 本课自测

1. 计划能解析为什么仍可能不能执行？
2. Step success 和 Goal completion 有什么区别？
3. RePlan 应保存哪些旧结果？
4. 为什么固定任务优先考虑 Workflow？
5. 比较策略时为什么要统一预算？

---

## 11. 延伸学习

- 图的拓扑排序；
- 有限状态机；
- HTN（Hierarchical Task Network）；
- ReAct、Plan-and-Solve、ReWOO 的控制差异；
- 工作流系统中的 durable execution 和 compensation。

---

## 12. AI 协作：让 AI 写状态机骨架，你负责计划语义

**本课起点**：从课程根目录使用 `配套代码/lesson-05-planning`，测试任务在 `公开数据/lesson-05-travel/`。

可以交给 AI：结构体与 serde、拓扑排序样板、状态机分支、fixture 加载、实验表。必须由你决定：什么是合法计划、Validator 检查什么、Step success 与 Goal completion 的差别、何时 RePlan、不同架构如何使用相同预算。

推荐 Prompt：

```text
请在现有 Plan/Step schema 上补全拓扑执行器，不新增“智能”组件。
先写环、未知工具、缺失依赖、失败传播的测试，再实现代码。
明确区分步骤成功与最终目标约束满足，并说明每个假设。
```

**AI 代码审查任务**：向 AI 生成的计划注入循环依赖和“每步成功但总价超预算”两个案例，说明为何只有执行器成功仍不等于任务成功。

---

## 作业：受约束的旅行协调任务

### 环境

讲师提供本地模拟工具：

- `search_transport`；
- `search_hotel`；
- `get_weather`；
- `calculate_cost`；
- `reserve`（模拟有副作用，不真实预订）。

所有数据来自 fixture，不依赖实时互联网。

### 三个实现

1. 固定 Workflow；
2. ReAct；
3. Plan-Execute-RePlan。

### 必测变化

- 预算突然减少；
- 某交通方式售罄；
- 酒店搜索超时；
- 模型生成循环依赖；
- 模型引用不存在的工具；
- 计划执行完但总价超预算；
- 用户中途改变日期。

### 指标

在至少 30 个任务上比较：

- goal completion rate；
- constraint violation rate；
- average model calls；
- average tool calls；
- average latency；
- unnecessary replan rate。

### 消融

关闭 `PlanValidator` 再跑同一测试，记录有多少非法计划进入执行阶段。

---

## 验收标准

- [ ] 计划依赖图能检测环；
- [ ] 未知工具不会进入执行；
- [ ] 步骤成功与目标完成分开验证；
- [ ] RePlan 有原因、有次数限制；
- [ ] 三种架构使用同一测试集比较；
- [ ] 即使 Workflow 获胜，也如实报告。

---

## 思考题

> 如果任务结构完全固定，只是每一步内部调用 LLM，为什么固定 Workflow 可能比 Planning Agent 更好？
