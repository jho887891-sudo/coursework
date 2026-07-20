//! # Lesson 4：Evidence 与 RAG — 让结论能够被核验
//!
//! 本 crate 实现：
//! - 确定性中文 tokenizer（字符 bigram baseline）
//! - 关键词检索与 Top-K 排序
//! - 结构化证据核验（存在性 / 一致性 / 支持性）
//! - 版本冲突检测与拒答
//! - 检索文档 Prompt Injection 防御
//! - 评测指标（Recall@K、Precision@K）
//!
//! ## 模块结构
//!
//! - [`tokenizer`] — 确定性中文分词
//! - [`retrieval`] — 关键词检索、评分、策略选择
//! - [`evidence`] — 证据核验、冲突检测、精确引用
//! - [`eval`]     — 评测指标与数据加载
//!
//! ## 快速开始
//!
//! ```rust
//! use lesson_04_evidence::*;
//!
//! let tokens = tokenize("投标保证金上限");
//! assert!(!tokens.is_empty());
//!
//! let passages = [
//!     Passage {
//!         source_id: "REG-2024".into(),
//!         locator: "§3.2".into(),
//!         text: "投标保证金不得超过项目预算金额的2%".into(),
//!         version: "v2".into(),
//!         effective_date: "2024-01-01".into(),
//!     },
//! ];
//!
//! let hits = search("保证金上限", &passages, 3);
//! assert_eq!(hits.len(), 1);
//!
//! let answer = verify("保证金上限为2%", &hits);
//! // answer.status 由核验函数判定
//! ```

mod evidence;
mod eval;
mod retrieval;
mod tokenizer;

// 核心数据结构 —— 定义在 lib.rs 顶层，所有模块共用
// ============================================================

/// 语料库中的一条原始段落。
///
/// 每条 passage 拥有可定位的唯一标识（source_id + locator）。
/// 学生不得把不同版本的同一段落合并，也不得丢弃 `version` 和 `effective_date`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passage {
    /// 来源标识（如 "REG-2024"）
    pub source_id: String,
    /// 段落定位符（如 "§3.2"）
    pub locator: String,
    /// 段落原文（不得改写或摘要）
    pub text: String,
    /// 版本号
    pub version: String,
    /// 生效日期
    pub effective_date: String,
}

/// 证据支持状态。
///
/// 注意：`Partial` 不是"及格线"，它表示模型/核验器确认
/// 部分语句有依据但存在 gap。上游决策者应据此决定是否求助人工。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceStatus {
    /// 证据充分支持结论
    Supported,
    /// 证据部分支持结论，存在 gap
    Partial,
    /// 证据不足以支持或否定结论
    Insufficient,
    /// 证据之间存在冲突（如不同版本/来源给出了相反结论）
    Conflicting,
}

/// 一条精确引用。
///
/// `quote` 必须直接从 `Passage::text` 按字符索引截取，
/// 不得由模型重写或"用自己的话概括"。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Citation {
    pub source_id: String,
    pub locator: String,
    pub quote: String,
}

/// 核验后的结构化回答。
///
/// 这是 Lesson 4 的最终输出格式。对比 Lesson 1 的纯文本 `answer`，
/// EvidenceAnswer 强制分离：
/// - 主张（claim）
/// - 依据（citations）
/// - 状态（status）
/// - 局限（limitations）
#[derive(Debug, Clone)]
pub struct EvidenceAnswer {
    pub claim: String,
    pub status: EvidenceStatus,
    pub citations: Vec<Citation>,
    pub limitations: Vec<String>,
}

// 重新导出所有公共 API
// ============================================================

pub use evidence::{document_authorizes_tool, quote_exact, verify};
pub use eval::{
    compute_metrics, evaluate_queries, find_failure_samples, load_eval_queries, load_passages,
    passage_id, precision_at_k, recall_at_k, EvalQuery, EvalSummary, ExperimentMetrics,
    QueryExperiment, SearchEval,
};
pub use retrieval::{
    decide_retrieval, score_tokens, search, search_with_hits, RetrieveDecision, RetrievalStrategy,
    SearchHit,
};
pub use tokenizer::tokenize;

// 内部单元测试（scaffold 编译检查）
// ============================================================

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_compiles() {
        assert!(true);
    }
}
