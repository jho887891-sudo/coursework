# AI 协作记录

## AI 参与

- 生成 Rust 数据结构、serde 派生和表驱动测试样板；
- 辅助实现稳定拓扑排序、fixture JSONL 加载和指标汇总；
- 补充错误分支、格式化和静态检查。

## 人工负责

- 定义“合法计划”的语义和 Validator 顺序；
- 决定 Step success 与 Goal completion 必须分离；
- 决定只有 Transient 允许有限重试；
- 设计 Blocked 传播、Tool 硬预算和 RePlan 上限；
- 定义哪些旧结果可在新计划中保留；
- 审查30条公开案例与危险副作用场景。

## 人工注入的反例

1. 循环依赖：JSON 合法但没有 Ready step；
2. 所有 Tool 成功但总价超预算：执行完成不等于目标完成；
3. 预订已提交但响应丢失：Timeout 不等于没有副作用；
4. 用户改变日期：旧酒店结果必须失效；
5. Planner 重复生成相同错误计划：必须受 max_replans 限制。
