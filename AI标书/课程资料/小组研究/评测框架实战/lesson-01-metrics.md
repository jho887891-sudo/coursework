# Day 1：评测指标体系 — 从 Confusion Matrix 到标书 5 维指标

> "Agent 准确率 85%"——这是怎么算出来的？用什么数据？跟什么比？今天你从最基础的 TP/FP/FN/TN 出发，推导出标书审核场景下真正有意义的评测体系。

---

## 学习目标

1. 从 Confusion Matrix 推导 P/R/F1/宏微平均的形式化定义
2. 实现多标签部分匹配的 Jaccard 评分
3. 设计标书审核的 5 维评测指标
4. 实现 Cohen's Kappa 标注一致性检验

---

## 核心概念

### 1. Confusion Matrix — 一切指标从这里出发

在你谈 F1 之前，必须理解 2×2 矩阵：

```
                    人工标注
                  Positive   Negative
               ┌──────────┬──────────┐
Agent  Positive │    TP    │    FP    │  ← Agent 告警了
输出   Negative │    FN    │    TN    │  ← Agent 没告警
               └──────────┴──────────┘

TP (True Positive): Agent 发现了问题，人工标注也认为是问题 → ✅
FP (False Positive): Agent 认为是问题，但人工标注说不是 → 误报
FN (False Negative): Agent 没发现，但人工标注认为有问题 → 漏检
TN (True Negative): Agent 没报，人工标注也认为没问题 → ✅
```

标书审核中的典型例子：

```
招标文件条款："投标人注册资本不低于 1 亿元"

Agent 输出: severity=critical, "注册资本限制构成排斥性条款"
人工标注: severity=critical, "排斥性条款——门槛过高"
→ TP ✅

Agent 输出: severity=warning, "建议审查工期要求的合理性"
人工标注: severity=none, "工期要求合理，无异常"
→ FP ❌（误报——Agent 过度敏感）

Agent 输出: (未发现任何问题)
人工标注: severity=critical, "要求投标人在东莞设立分支机构——明显的排斥性"
→ FN ❌（漏检——Agent 错过了关键问题）
```

### 2. P/R/F1 — 三个指标，一个都不能少

#### Precision（精确率）

$$P = \frac{TP}{TP + FP}$$

"Agent 告警的所有问题中，有多少是真的？"

- P=0.95 → Agent 说有问题时，95% 的时候确实有问题 → 可信任
- P=0.30 → Agent 说有问题时，70% 是误报 → 用户逐渐忽略告警

标书审核中，P 低意味着用户被大量假告警淹没。"狼来了"效应——用户不再信任系统。

#### Recall（召回率）

$$R = \frac{TP}{TP + FN}$$

"所有真实存在的问题中，Agent 发现了多少？"

- R=0.95 → 几乎不漏 → 用户放心
- R=0.60 → 40% 的问题被漏掉 → 废标风险

标书审核中，R 低意味着用户以为标书没问题，但实际藏着废标陷阱。漏检比误报更危险——误报最多浪费用户时间，漏检可能导致废标。

#### F1 — 调和平均

$$F_1 = 2 \cdot \frac{P \cdot R}{P + R}$$

为什么是调和平均而不是算术平均？因为 F1 偏向更低的值：

```
P=0.95, R=0.50 → F1 = 2×0.95×0.50/(0.95+0.50) = 0.655
算术平均 = (0.95+0.50)/2 = 0.725

→ F1 惩罚了"P 高但 R 低"的失衡——这正是标书审核要避免的！
  Agent 只报告它"非常确定"的问题（P 高但 R 低 = 漏检严重）
```

#### 宏平均 vs 微平均

多类别场景（资质/业绩/报价/工期/安全/环保 6 类问题），有两种汇总方式：

```
宏平均 (Macro): 每个类别算 P_i 和 R_i → 取平均
微平均 (Micro): 所有类别的 TP/FP/FN 汇总 → 算一个全局 P 和 R

类别分布：资质(80 个 TP)/ 业绩(15 个 TP)/ 安全(5 个 TP)

微平均 P = (80 + 15 + 5) / (80 + 10 + 15 + 3 + 5 + 2) ≈ 0.87
  但这是由"资质"类主导的——你误以为 Agent 在所有类别都很准！

宏平均 P = (0.89 + 0.83 + 0.71) / 3 ≈ 0.81
  安全类拖了后腿——这才是真实的全貌。
```

**标书审核必须用宏平均**。类别极不均衡——资质问题远多于安全问题。微平均会被大类主导，掩盖小类的劣化。

---

### 3. 多标签部分匹配

标书审核的一个条款可以有**多个审核结论**——不是单选题：

```
条款："投标人注册资本不低于 1 亿元且须在东莞设立分支机构"

人工标注: [
  {type: "排斥性-注册资本过高", severity: "critical"},
  {type: "排斥性-地域限制", severity: "critical"}
]

Agent 输出: [
  {type: "排斥性-注册资本过高", severity: "critical"}
]
```

Agent 发现了注册资本问题（正确），但漏了地域限制（漏检）。不是简单的"对/错"——而是"对了几成"。

**Jaccard 部分匹配**：

$$J(A, G) = \frac{|A \cap G|}{|A \cup G|} = \frac{匹配到的标注数}{Agent输出数 + 标注数 - 匹配数}$$

```
A = {排斥性-注册资本过高}
G = {排斥性-注册资本过高, 排斥性-地域限制}
匹配 = {排斥性-注册资本过高}（按 finding_type 的语义等价判定）

J = 1 / (1 + 2 - 1) = 1/2 = 0.5 → 只对了一半
```

在多标签场景中，用 Jaccard 替代严格的 exact match——"对了一半"比"完全错了"更准确地反映了 Agent 的表现。

---

### 4. Cohen's Kappa — 纠正"偶然一致"

两个人标注 100 条数据，一致率 85%。看起来不错。但如果 90% 的条款都没有问题（Negative），抛硬币("都标没问题")也能达到 90% 的一致率。

Cohen's Kappa 纠正了偶然因素：

$$\kappa = \frac{P_o - P_e}{1 - P_e}$$

其中 $P_o$ = 观察到的一致率，$P_e$ = 偶然期望一致率。

```
Po = 0.85
Pe = 0.65（两个标注者各自随机猜，期望一致率 65%——因为有大量"无问题"标签）

κ = (0.85 - 0.65) / (1 - 0.65) = 0.20 / 0.35 = 0.571

解读：
  κ < 0:    比随机还差
  κ 0-0.2:  slight
  κ 0.2-0.4: fair
  κ 0.4-0.6: moderate
  κ 0.6-0.8: substantial
  κ 0.8-1.0: almost perfect

0.57 → 勉强 moderate，还有很大的对齐空间。
```

---

## 动手

### 任务 1：手写 P/R/F1 + 宏微平均

```rust
pub struct Metrics {
    tp: usize, fp: usize, fn: usize,
}

impl Metrics {
    pub fn precision(&self) -> f64;
    pub fn recall(&self) -> f64;
    pub fn f1(&self) -> f64;
}

pub fn macro_average(class_metrics: &[Metrics]) -> (f64, f64, f64);
pub fn micro_average(class_metrics: &[Metrics]) -> (f64, f64, f64);
pub fn jaccard_multilabel(agent_findings: &[Finding], gold_findings: &[Finding]) -> f64;
```

### 任务 2：实现 Cohen's Kappa

```rust
fn cohens_kappa(annotator_a: &[Label], annotator_b: &[Label], n_categories: usize) -> f64;
fn krippendorffs_alpha(annotations: &[Vec<Label>]) -> f64; // >= 3 annotators
```

### 任务 3：标书 5 维指标设计

设计 5 个指标（准确性/完整性/可追溯性/实用性/效率）的量化方法。每个指标写清楚：数据来源是什么、计算公式是什么、为什么选这个而不是别的。

---

## 验收标准

- [ ] P/R/F1 + 宏微平均 + Jaccard 全部正确实现
- [ ] Cohen's Kappa 在测试数据上输出正确（与 sklearn 对照）
- [ ] 5 维指标设计文档每个指标 ≥ 150 字说明

---

## 思考题

1. F1 是调和平均，意味着 P 和 R 同等重要。在标书审核中，漏检（低 Recall）和误报（低 Precision）哪个更严重？如果 Recall 是 Precision 的 2 倍重要，你怎么设计加权 F_beta？
2. 如果标注者 A 总是标"critical"，标注者 B 总是标"warning"——他们的 Kappa 会是多少？这反映了 Kappa 的什么局限？
3. Jaccard 部分匹配认为 "排斥性-注册资本过高" 和 "排斥性-门槛过高" 是匹配还是不匹配？你怎么定义"语义等价"？

---

## 与标书审核项目的关系

课程用小样本理解 P/R/F1、宏平均、部分匹配和 Kappa。正式项目扩大到 50 份或更多 Benchmark 时仍使用这些方法，但阈值需要结合任务风险、样本量和混淆矩阵解释，不能只凭一个 Kappa 数字决定 Gold Standard。
