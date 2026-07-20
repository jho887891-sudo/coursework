//! 关键词检索与排序模块。
//!
//! 实现三种检索策略的公共基础设施：
//! 1. 基于 token 重叠的评分函数
//! 2. Top-K 排序检索
//! 3. 三种检索策略的枚举与执行入口
//!
//! ## 评分原理
//!
//! 使用 token 命中数的 TF 加权和作为相关性分数：
//!
//! ```text
//! score(query, passage) = Σ term_frequency(token, passage) for each query_token
//! ```
//!
//! 这不是 BM25，但建立了可评测的 baseline。学生可在同一 tokenizer 上替换
//! 评分函数（BM25、embedding cosine 等）而无需改动上层 Evidence 逻辑。

use crate::tokenize;
use crate::Passage;

// ============================================================================
// 检索策略
// ============================================================================

/// 三种检索策略，对应 Lecture 8 的 Checkpoint B/C/D。
///
/// 不同策略的区别在于 **是否检索** 和 **何时检索**，而非检索本身如何实现。
/// 检索内核（tokenizer + scorer + Top-K）在所有策略间保持一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetrievalStrategy {
    /// 总是检索：每个 query 固定检索 Top-K
    AlwaysRetrieve,
    /// 规则路由：命中关键词（依据/规则/条款/比例/期限等）才检索
    RuleRouter,
    /// Agentic 检索：由决策层根据当前证据决定是否继续检索
    AgenticRetrieve,
}

/// 检索策略的决策结果。
#[derive(Debug, Clone)]
pub enum RetrieveDecision {
    /// 应检索，使用改写后的 query（可能与原始不同）
    Search { query: String },
    /// 不应检索
    Skip { reason: String },
}

/// 根据策略和当前状态判断是否应检索。
///
/// - `AlwaysRetrieve`：无条件返回 `Search`
/// - `RuleRouter`：检查是否命中触发词
/// - `AgenticRetrieve`：预留接口，当前回退为 `AlwaysRetrieve`
pub fn decide_retrieval(
    strategy: RetrievalStrategy,
    query: &str,
    _evidence_so_far: &[crate::Citation],
) -> RetrieveDecision {
    match strategy {
        RetrievalStrategy::AlwaysRetrieve => RetrieveDecision::Search {
            query: query.to_owned(),
        },
        RetrievalStrategy::RuleRouter => {
            if should_retrieve_by_rule(query) {
                RetrieveDecision::Search {
                    query: query.to_owned(),
                }
            } else {
                RetrieveDecision::Skip {
                    reason: "未命中检索触发词".to_owned(),
                }
            }
        }
        RetrievalStrategy::AgenticRetrieve => {
            // Agentic 检索需要模型参与决策；本课 baseline 回退为总是检索
            RetrieveDecision::Search {
                query: query.to_owned(),
            }
        }
    }
}

/// 规则路由的触发词集合。
///
/// 包含中文招投标领域常见的关键词。命中任意一词即触发检索。
fn should_retrieve_by_rule(query: &str) -> bool {
    let triggers = [
        "依据", "规则", "条款", "比例", "期限", "保证金", "资格", "条件",
        "标准", "要求", "法规", "规定", "限额", "上限", "资质", "程序",
        "评审", "投标", "合同", "付款", "履约", "违约",
        "证据", "原文", "出处", "根据", "适用",
    ];
    triggers.iter().any(|&t| query.contains(t))
}

// ============================================================================
// 检索命中
// ============================================================================

/// 单条检索结果，包含定位信息、原文和相关性分数。
///
/// 与 `Passage` 不同：`SearchHit` 附带了本次检索的 `score`。
/// `score` 用于排序和截断 Top-K，不应对上层 Evidence 做硬阈值判断。
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub source_id: String,
    pub locator: String,
    pub text: String,
    pub version: String,
    pub effective_date: String,
    /// 相关性分数（越高越相关）
    pub score: f32,
    /// 本次检索使用的策略
    pub retrieved_by: String,
}

impl SearchHit {
    /// 从 Passage 和分数构造 SearchHit。
    pub fn from_passage(passage: &Passage, score: f32, strategy: RetrievalStrategy) -> Self {
        Self {
            source_id: passage.source_id.clone(),
            locator: passage.locator.clone(),
            text: passage.text.clone(),
            version: passage.version.clone(),
            effective_date: passage.effective_date.clone(),
            score,
            retrieved_by: format!("{:?}", strategy),
        }
    }
}

// ============================================================================
// 评分函数
// ============================================================================

/// 计算 query 对单条 passage 的相关性分数。
///
/// 当前实现：query 中每个 token 在 passage 中出现的次数之和。
/// 这是一个 TF（词频）baseline，不包含 IDF 权重和长度归一化。
///
/// # 参数
/// - `query_tokens`：对 query 调用 `tokenize()` 的结果
/// - `passage_text`：passage 的原文文本
///
/// # 返回
/// 非负浮点数，0 表示无任何 token 命中。
pub fn score_tokens(query_tokens: &[String], passage_text: &str) -> f32 {
    if query_tokens.is_empty() {
        return 0.0;
    }

    let passage_lower = passage_text.to_lowercase();

    let hits: usize = query_tokens
        .iter()
        .filter(|token| {
            let token_lower = token.to_lowercase();
            passage_lower.contains(&token_lower)
        })
        .count();

    // 归一化：命中数 / query token 数，范围 [0, 1]
    hits as f32 / query_tokens.len() as f32
}

/// 计算 query 对一条 passage 的 BM25 风格的分数（简化版）。
///
/// 这是 `score_tokens` 的增强版，额外考虑了：
/// - 命中 token 在 passage 中的频率
/// - 简单的长度惩罚
///
/// 学生实验时可在 checkpoint E 替换此函数。
#[allow(dead_code)]
pub fn score_bm25_simple(
    query_tokens: &[String],
    passage_text: &str,
    avg_passage_len: f32,
    total_passages: usize,
) -> f32 {
    let k1 = 1.2;
    let b = 0.75;
    let passage_len = passage_text.chars().count() as f32;

    let passage_lower = passage_text.to_lowercase();

    let mut score = 0.0;
    for token in query_tokens {
        let token_lower = token.to_lowercase();
        let tf = passage_lower.matches(&token_lower).count() as f32;
        if tf == 0.0 {
            continue;
        }

        // 简化的 TF 饱和
        let tf_saturated = (tf * (k1 + 1.0)) / (tf + k1 * (1.0 - b + b * passage_len / avg_passage_len.max(1.0)));

        // 简化的 IDF：假设该 token 在一半的文档中出现
        let idf = ((total_passages as f32 + 0.5) / (total_passages as f32 / 2.0 + 0.5)).ln();

        score += tf_saturated * idf.max(0.0);
    }

    score
}

// ============================================================================
// 检索入口
// ============================================================================

/// 在 passages 中检索与 query 最相关的 Top-K 条。
///
/// # 算法
/// 1. 对 query 分词
/// 2. 对每条 passage 计算相关性分数
/// 3. 按分数降序排列
/// 4. 取前 K 条（分数 > 0 的优先）
///
/// # 注意
/// - 分数为 0 的 passage 不会出现在结果中（节省下游工作量）
/// - K 为 0 时返回空
/// - 结果按分数降序，同分时保持原顺序（stable sort）
pub fn search<'a>(query: &str, passages: &'a [Passage], k: usize) -> Vec<&'a Passage> {
    if k == 0 || query.trim().is_empty() {
        return vec![];
    }

    let query_tokens = tokenize(query);
    if query_tokens.is_empty() {
        return vec![];
    }

    // 计算每条 passage 的分数
    let mut scored: Vec<(f32, usize)> = passages
        .iter()
        .enumerate()
        .map(|(idx, passage)| {
            let s = score_tokens(&query_tokens, &passage.text);
            (s, idx)
        })
        .collect();

    // 按分数降序排列（stable：同分保持原序）
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // 取 Top-K（跳过分数为 0 的）
    let top: Vec<&Passage> = scored
        .iter()
        .filter(|(score, _)| *score > 0.0)
        .take(k)
        .map(|(_, idx)| &passages[*idx])
        .collect();

    top
}

/// 带元数据的检索，返回 `SearchHit` 列表而非 `&Passage` 列表。
///
/// 适用于需要分数和策略标记的上游逻辑（如实验统计）。
pub fn search_with_hits(
    query: &str,
    passages: &[Passage],
    k: usize,
    strategy: RetrievalStrategy,
) -> Vec<SearchHit> {
    if k == 0 || query.trim().is_empty() {
        return vec![];
    }

    let query_tokens = tokenize(query);
    if query_tokens.is_empty() {
        return vec![];
    }

    let mut scored: Vec<(f32, usize)> = passages
        .iter()
        .enumerate()
        .map(|(idx, passage)| {
            let s = score_tokens(&query_tokens, &passage.text);
            (s, idx)
        })
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    scored
        .iter()
        .filter(|(score, _)| *score > 0.0)
        .take(k)
        .map(|(score, idx)| {
            SearchHit::from_passage(&passages[*idx], *score, strategy)
        })
        .collect()
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn p(id: &str, loc: &str, text: &str) -> Passage {
        Passage {
            source_id: id.into(),
            locator: loc.into(),
            text: text.into(),
            version: "v1".into(),
            effective_date: "2026-01-01".into(),
        }
    }

    #[test]
    fn score_zero_for_empty_query() {
        assert_eq!(score_tokens(&[], "anything"), 0.0);
    }

    #[test]
    fn score_positive_for_full_match() {
        let tokens = tokenize("材料迟交");
        let score = score_tokens(&tokens, "材料迟交时转人工复核");
        assert!(score > 0.0);
    }

    #[test]
    fn score_higher_for_more_overlap() {
        let tokens = tokenize("迟交材料");
        let good = score_tokens(&tokens, "材料迟交时转人工复核");
        let bad = score_tokens(&tokens, "付款义务应按约定履行");
        assert!(good > bad);
    }

    #[test]
    fn search_finds_relevant_passage() {
        let ps = [p("S1", "A-1", "材料迟交时转人工复核")];
        let results = search("迟交材料", &ps, 3);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_id, "S1");
    }

    #[test]
    fn search_respects_k() {
        let ps = [
            p("S1", "A-1", "材料迟交时转人工复核"),
            p("S2", "A-2", "材料缺失需补交"),
            p("S3", "A-3", "签字不全转人工"),
        ];
        let results = search("材料", &ps, 2);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn search_returns_nothing_for_empty_query() {
        let ps = [p("S1", "A-1", "材料迟交时转人工复核")];
        assert!(search("", &ps, 3).is_empty());
        assert!(search("   ", &ps, 3).is_empty());
    }

    #[test]
    fn search_returns_nothing_when_k_is_zero() {
        let ps = [p("S1", "A-1", "材料迟交时转人工复核")];
        assert!(search("迟交", &ps, 0).is_empty());
    }

    #[test]
    fn zero_score_passages_are_excluded() {
        let ps = [p("S1", "A-1", "付款义务应按约定履行")];
        let results = search("迟交材料", &ps, 10);
        assert!(results.is_empty());
    }

    #[test]
    fn rule_router_triggers_on_keywords() {
        assert!(matches!(
            decide_retrieval(RetrievalStrategy::RuleRouter, "投标保证金上限", &[]),
            RetrieveDecision::Search { .. }
        ));
    }

    #[test]
    fn rule_router_skips_without_trigger() {
        assert!(matches!(
            decide_retrieval(RetrievalStrategy::RuleRouter, "今天天气怎么样", &[]),
            RetrieveDecision::Skip { .. }
        ));
    }

    #[test]
    fn always_retrieve_never_skips() {
        assert!(matches!(
            decide_retrieval(RetrievalStrategy::AlwaysRetrieve, "任意问题", &[]),
            RetrieveDecision::Search { .. }
        ));
    }
}
