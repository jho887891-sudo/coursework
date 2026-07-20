# Day 2：Benchmark 构建 — 从分层抽样到 Gold Standard

> Day 1 你学会了怎么算指标。今天你学会怎么造"标准答案"——没有标准答案，指标再漂亮也是自欺欺人。

---

## 学习目标

1. 设计分层抽样策略（项目类型/风险密度/文档完整度）
2. 编写标注协议（标注单元/标签体系/边界 case/示例）
3. 计算 Inter-Annotator Agreement (IAA) 并诊断分歧来源
4. 实施 Gold Standard 仲裁流程

---

## 核心概念

### 1. 分层抽样 — 正式 Benchmark 不能随便随机选

#### 为什么随机抽样不够

100 份招标文件中，70 份是施工招标，20 份货物采购，10 份服务招标。如果随机抽 50 份：

```
随机抽样的期望分布：
  施工: 35 份 (70%)
  货物: 10 份 (20%)
  服务: 5 份 (10%)

问题：服务招标只有 5 份 → Benchmark 在服务类上的评测结果极不稳定
  (5 份的 P/R 方差远大于 35 份的方差 → 服务类的 CI 极宽 → 无法判断是否退化)
```

#### 分层抽样策略

```rust
struct StratifiedSampler {
    strata: Vec<Stratum>,
}

struct Stratum {
    name: String,           // "施工招标-高风险"
    population: usize,      // 总体中该层的数量
    target_sample: usize,   // 该层目标抽样数
    variables: Vec<String>, // 分层变量
}

impl StratifiedSampler {
    fn sample<R: Rng>(&self, pool: &[Document], rng: &mut R) -> Vec<Document> {
        // 1. 按分层变量将 pool 分配到各层
        // 2. 每层内独立随机抽样（固定 seed）
        // 3. 如果某层样本不足 → 从相邻层"借用"或标记 undersampled
    }
}
```

分层变量：
- 第一层：项目类型（施工/货物/服务）。施工降权到 50%，货物 30%，服务 20%
- 第二层：风险密度（用现有 Agent 预标注）。高(>10 issues) 30% / 中(5-10) 40% / 低(<5) 30%
- 第三层：文档完整度（完整/部分/简版）

最终 50 份的分配：
```
施工: 25 份 (高10 + 中10 + 低5)
货物: 15 份 (高5 + 中6 + 低4)
服务: 10 份 (高4 + 中4 + 低2)
```

种子固定：`rng.seed(42)`——确保其他人能复现你的抽样结果。

---

### 2. 标注协议 — 让两个人标出同样的结果

#### 协议五要素

**1. 标注单元定义**

"标注单元 = 一个条款（clause）"还是"一个审核发现（finding）"？

标书审核推荐后者——以 finding 为单元。因为一个条款可能对应多个 finding（如"注册资本≥1亿"既是排斥性条款，又是资质门槛过高）。标注工具应该允许标注者在同一个 clause 上标注多个 finding。

**2. 多级标签体系**

```
finding_type（必选）:
  - 资质要求问题           (qualification)
  - 业绩要求问题           (experience)
  - 报价要求问题           (pricing)
  - 工期要求问题           (timeline)
  - 安全/环保要求问题      (safety)
  - 排斥性/歧视性条款      (discrimination)
  - 格式/程序问题          (procedural)

severity（必选）:
  - critical: 可能导致废标的致命问题
  - warning: 存在风险但可协商
  - info: 提示性信息，不一定有问题

law_ref（条件必选——如果 finding_type ∈ {qualification, discrimination}）:
  - 引用法规名称 + 条款编号
```

**3. 边界 case 规则**

标注协议必须预判歧义场景：

| 边界场景 | 标注规则 |
|----------|----------|
| 条款同时属于"资质问题"和"排斥性条款" | 标注两个 finding，各自独立 |
| 条款可能有问题但不确定 | 标注 severity=info + note="不确定，需法律确认" |
| 条款引用的法规条款编号不明显 | 标注 law_ref=null + note="法规依据待查" |
| 条款在页面边界——一半在第 5 页一半在第 6 页 | 标注在 clause_id 所在的页面（按 clause 起始位置） |

**4. 标注示例**

协议中至少包含 5 个"对"和 5 个"错"的标注示例——标注者看了示例就知道怎么标。

**5. 一致性检查点**

标注满 5 条后跑一次 IAA。如果 Kappa < 0.6 → 停止标注，重新对齐理解。

---

### 3. IAA 与 Gold Standard 构建

#### 双盲标注流程

```
第一轮：两位标注者独立标注 10 条（双盲）
  → 计算 Kappa
  → Kappa < 0.8 → 讨论分歧，更新标注协议
  → 重新标注 10 条

第二轮：两位标注者继续独立标注 40 条
  → 每 10 条跑一次 Kappa
  → 如果 Kappa 持续下降 → 标注疲劳 → 休息或换人

仲裁（对于分歧的条款）：
  → 仲裁者（独立第三人，领域专家）审查双方标注
  → 选出最终的 Gold Standard
  → 记录仲裁理由 + 原始分歧
```

#### Kappa 不够——看混淆矩阵

```
Kappa = 0.82（看起来不错）

但混淆矩阵显示：
            标注者 B
            crit  warn  info  none
标注者 A  crit  45     8     0     0
         warn   5    30    10     2
         info   0     5    20     5
         none   0     0     3    67

→ "critical vs warning" 之间有 8+5=13 条分歧
→ Per-class Kappa:
   critical: κ=0.91 ✅
   warning:  κ=0.62 ⚠️  ← 问题在这！
```

标注者 A 和 B 在"这算 warning 还是 critical"上有显著分歧 → 标签体系的 severity 定义需要更精确。

---

## 动手

### 任务 1：实现分层抽样器

```rust
let mut sampler = StratifiedSampler::new(
    vec![
        ("project_type", vec!["施工", "货物", "服务"], vec![0.5, 0.3, 0.2]),
        ("risk_density", vec!["高", "中", "低"], vec![0.3, 0.4, 0.3]),
    ],
);
let sample = sampler.sample(&pool, 15, seed=42); // 课程 Demo
```

### 任务 2：计算 IAA

用讲师提供的双人标注数据（20 条），计算：
- 整体 Cohen's Kappa
- 每个 severity 类别的 per-class Kappa
- 混淆矩阵 → 找出最大分歧的类别对

### 任务 3：仲裁一条分歧

在标注数据中找一条"标注者 A 和 B 不一致"的条款 → 你自己当仲裁者 → 做出最终判定 → 写 100 字仲裁理由。

---

## 验收标准

- [ ] 用 10～20 条小样本演示分层抽样并解释局限
- [ ] Cohen's Kappa + per-class Kappa + 混淆矩阵全部正确
- [ ] 至少分析出 1 个"标注协议不够清晰导致的分歧"

---

## 思考题

1. 如果把分层变量从 2 个增加到 4 个→最底层的子层可能只有 1-2 个样本。这个抽样还有意义吗？你怎么办？
2. Krippendorff's Alpha 支持 3 个以上的标注者。在什么情况下值得用 3 个标注者？（提示：考虑 IAA 的统计功效）
3. Gold Standard 仲裁中，如果两个标注者的理由都合理但不能共存→这条条款应该被排除出 Gold Standard 吗？还是标记为"存在不确定性"？

---

## 与标书审核项目的关系

课程先用小样本理解抽样、标注协议、IAA 和仲裁。正式项目可以扩展到 50 份或更多文档，但课程结果不能直接当作 Gold Standard；样本扩大后需要重新检查分层覆盖、标注一致性和不确定性。
