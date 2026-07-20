# Lesson 4：实现检索与证据验证

确定性中文 tokenizer（bigram baseline）、关键词检索与 Top-K 排序、来源定位、引用核验（存在性 / 一致性 / 支持性）、版本冲突检测、拒答、检索文档 Prompt Injection 防御，以及 Recall@K / Precision@K 评测。

## 快速运行

```powershell
cargo test -p lesson-04-evidence --test acceptance
```

## 演示

```powershell
# 运行全部单元测试与验收测试（不调用真实模型）
cargo test -p lesson-04-evidence

# 查看评测指标示例（需要 JSONL 数据文件）
cargo test -p lesson-04-evidence -- eval --nocapture
```

## 本课证明什么

1. **主题相关 ≠ 证据支持**：`topical_overlap_does_not_prove_claim` 测试验证同一个主题词但结论词不同时，系统判定为 Insufficient 而非 Supported。
2. **冲突不静默丢弃**：不同来源/版本对同一主题给出不同结论时，系统标记为 Conflicting 并保留双方原文。
3. **数据不是权限**：检索文档中的任何指令都不能扩大 Runtime 的工具权限。
4. **引用可溯源**：每条 citation 保留 `source_id + locator + quote`，quote 从原文按字符索引精确截取。

## 模块结构

| 文件 | 内容 |
|---|---|
| `src/lib.rs` | 公共数据结构（Passage / EvidenceStatus / Citation / EvidenceAnswer）与模块 re-export |
| `src/tokenizer.rs` | 字符 bigram 确定性中文分词 |
| `src/retrieval.rs` | 关键词评分、Top-K 检索、三种检索策略（Always / RuleRouter / Agentic） |
| `src/evidence.rs` | 证据核验、冲突检测、精确引用提取、文档 Prompt Injection 防御 |
| `src/eval.rs` | JSONL 数据加载、Recall@K / Precision@K 计算、评测运行与汇总 |

## 代码阅读顺序

1. `src/lib.rs` — 理解数据结构
2. `src/tokenizer.rs` — 分词如何工作（确定性与 bigram 选择的原因见注释）
3. `src/retrieval.rs` — 评分函数与三种策略
4. `src/evidence.rs` — 核验流程（重点：`verify()` 函数中的重叠率与结论缺失检测）
5. `src/eval.rs` — 评测闭环
6. `tests/acceptance.rs` — 24 个验收测试作为规格说明

## 关键设计决策

### 为什么用 Bigram 而不是 jieba？

- Bigram 不需要词典、不需要训练、不依赖外部库；
- 在中文 IR 中是经典强 baseline；
- 完全确定 —— 同一输入永远同一输出，三种策略可比；
- 学生理解后可替换为任意分词器，评测框架不变。

### 为什么 token 去重？

- 提升评分稳定性：重复 token（如 "材料材料材料"）不扭曲分数；
- 去重后 `matched / total` 仍保持语义：衡量的是"claim 中有多少不同的词被 passage 覆盖"。

### 为什么 `document_authorizes_tool` 不是空函数？

它的存在是一份文档化声明。代码审查者可通过 grep 找到所有曾经尝试从数据获取权限的位置。永远返回 `false` 确保将来无人能以"特殊情况"为由在此加入条件判断。

## 重要边界

- 本课不是生产级 RAG 系统，不实现 embedding 检索、hybrid search 或 reranking；
- tokenizer 不处理繁简体转换，学生实验时应使用统一文本；
- 冲突检测基于来源 ID + 版本 + token 差异，不进行语义理解；
- `verify()` 的判定阈值（0.8 / 0.5）是教学参数，学生应在实验中调参并观察对 Precision/Recall 的影响。
