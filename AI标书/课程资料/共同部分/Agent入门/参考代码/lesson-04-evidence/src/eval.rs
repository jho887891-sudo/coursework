//! 检索评测模块 —— Recall@K、Precision@K、数据加载。
//!
//! 检索系统不能靠"看起来搜到了"来评估。本模块提供量化指标，
//! 使三种检索策略（Always/Route/Agentic）在同一评测集上可比。
//!
//! ## 评测数据格式
//!
//! 评测文件 `eval_queries.jsonl` 每行一条：
//!
//! ```json
//! {"query":"投标保证金上限","relevant_ids":["REG-2024#§3.2"],"answerable":true}
//! ```
//!
//! - `query`：检索查询
//! - `relevant_ids`：人工标注的相关段落 ID（格式：`source_id#locator`）
//! - `answerable`：该问题是否可基于现有语料回答

use crate::{EvidenceStatus, Passage};
use serde::Deserialize;
use std::path::Path;

// ============================================================================
// 评测数据加载
// ============================================================================

/// 单条评测查询。
#[derive(Debug, Clone, Deserialize)]
pub struct EvalQuery {
    pub query: String,
    pub relevant_ids: Vec<String>,
    pub answerable: bool,
}

/// 语料段落（JSONL 加载格式）。
///
/// 与 `Passage` 结构一致，但使用 serde 反序列化。
#[derive(Debug, Clone, Deserialize)]
struct PassageRecord {
    source_id: String,
    locator: String,
    text: String,
    version: String,
    effective_date: String,
}

impl From<PassageRecord> for Passage {
    fn from(r: PassageRecord) -> Self {
        Self {
            source_id: r.source_id,
            locator: r.locator,
            text: r.text,
            version: r.version,
            effective_date: r.effective_date,
        }
    }
}

/// 从 JSONL 文件加载评测查询。
///
/// 每行一条 JSON，格式见 [`EvalQuery`]。
///
/// # 错误处理
/// 无法解析的行会被跳过（打印警告到 stderr），不会导致整体失败。
/// 这允许评测集中混入手工注释行。
pub fn load_eval_queries(path: &Path) -> Result<Vec<EvalQuery>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("无法读取评测文件 {}: {}", path.display(), e))?;

    let mut queries = Vec::new();
    for (line_no, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match serde_json::from_str::<EvalQuery>(line) {
            Ok(q) => queries.push(q),
            Err(e) => {
                eprintln!(
                    "警告: 跳过第 {} 行无法解析的评测条目: {}",
                    line_no + 1,
                    e
                );
            }
        }
    }

    Ok(queries)
}

/// 从 JSONL 文件加载语料 passages。
///
/// 每行一条 JSON，格式见 [`PassageRecord`]。
pub fn load_passages(path: &Path) -> Result<Vec<Passage>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("无法读取语料文件 {}: {}", path.display(), e))?;

    let mut passages = Vec::new();
    for (line_no, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match serde_json::from_str::<PassageRecord>(line) {
            Ok(r) => passages.push(r.into()),
            Err(e) => {
                eprintln!(
                    "警告: 跳过第 {} 行无法解析的语料条目: {}",
                    line_no + 1,
                    e
                );
            }
        }
    }

    Ok(passages)
}

// ============================================================================
// 评测指标
// ============================================================================

/// 单次检索的评测结果。
#[derive(Debug, Clone)]
pub struct SearchEval {
    pub query: String,
    /// 检索到的 passage ID 列表
    pub retrieved_ids: Vec<String>,
    /// 标注的相关 passage ID 列表
    pub relevant_ids: Vec<String>,
    /// Recall@K
    pub recall: f32,
    /// Precision@K
    pub precision: f32,
    /// 该 query 是否可回答
    pub answerable: bool,
}

/// 在全部 query 上汇总的评测指标。
#[derive(Debug, Clone)]
pub struct EvalSummary {
    /// 总 query 数
    pub total_queries: usize,
    /// 平均 Recall@K
    pub avg_recall: f32,
    /// 平均 Precision@K
    pub avg_precision: f32,
    /// 可回答 query 中的平均 Recall@K
    pub avg_recall_answerable: f32,
    /// 不可回答 query 数
    pub unanswerable_count: usize,
}

/// 计算 Recall@K：相关文档中被检索出的比例。
///
/// ```text
/// Recall@K = |retrieved_ids ∩ relevant_ids| / |relevant_ids|
/// ```
///
/// 当 `relevant_ids` 为空时，Recall 定义为 1.0（没有相关文档需要检索）。
pub fn recall_at_k(retrieved_ids: &[String], relevant_ids: &[String]) -> f32 {
    if relevant_ids.is_empty() {
        return 1.0;
    }

    let retrieved_set: std::collections::HashSet<&String> = retrieved_ids.iter().collect();
    let hits = relevant_ids
        .iter()
        .filter(|id| retrieved_set.contains(id))
        .count();

    hits as f32 / relevant_ids.len() as f32
}

/// 计算 Precision@K：检索结果中真正相关的比例。
///
/// ```text
/// Precision@K = |retrieved_ids ∩ relevant_ids| / |retrieved_ids|
/// ```
///
/// 当 `retrieved_ids` 为空时，Precision 定义为 0.0（没搜到任何东西）。
pub fn precision_at_k(retrieved_ids: &[String], relevant_ids: &[String]) -> f32 {
    if retrieved_ids.is_empty() {
        return 0.0;
    }

    let relevant_set: std::collections::HashSet<&String> = relevant_ids.iter().collect();
    let hits = retrieved_ids
        .iter()
        .filter(|id| relevant_set.contains(id))
        .count();

    hits as f32 / retrieved_ids.len() as f32
}

/// 构造 passage 的 ID 字符串（`source_id#locator` 格式）。
pub fn passage_id(passage: &Passage) -> String {
    format!("{}#{}", passage.source_id, passage.locator)
}

/// 在评测集上运行检索评测。
///
/// # 参数
/// - `queries`：评测查询列表
/// - `passages`：语料库
/// - `k`：Top-K 参数
/// - `search_fn`：检索函数，接受 query 和 passages，返回检索到的 passages
pub fn evaluate_queries<'a, F>(
    queries: &[EvalQuery],
    passages: &'a [Passage],
    k: usize,
    search_fn: F,
) -> Vec<SearchEval>
where
    F: Fn(&str, &'a [Passage], usize) -> Vec<&'a Passage>,
{
    queries
        .iter()
        .map(|eq| {
            let hits = search_fn(&eq.query, passages, k);
            let retrieved_ids: Vec<String> = hits.iter().map(|p| passage_id(p)).collect();
            let r = recall_at_k(&retrieved_ids, &eq.relevant_ids);
            let p = precision_at_k(&retrieved_ids, &eq.relevant_ids);

            SearchEval {
                query: eq.query.clone(),
                retrieved_ids,
                relevant_ids: eq.relevant_ids.clone(),
                recall: r,
                precision: p,
                answerable: eq.answerable,
            }
        })
        .collect()
}

/// 汇总评测结果。
#[allow(dead_code)]
pub fn summarize(evals: &[SearchEval]) -> EvalSummary {
    let total = evals.len();
    if total == 0 {
        return EvalSummary {
            total_queries: 0,
            avg_recall: 0.0,
            avg_precision: 0.0,
            avg_recall_answerable: 0.0,
            unanswerable_count: 0,
        };
    }

    let avg_recall = evals.iter().map(|e| e.recall).sum::<f32>() / total as f32;
    let avg_precision = evals.iter().map(|e| e.precision).sum::<f32>() / total as f32;

    let answerable: Vec<&SearchEval> = evals.iter().filter(|e| e.answerable).collect();
    let avg_recall_answerable = if answerable.is_empty() {
        0.0
    } else {
        answerable.iter().map(|e| e.recall).sum::<f32>() / answerable.len() as f32
    };

    let unanswerable_count = evals.iter().filter(|e| !e.answerable).count();

    EvalSummary {
        total_queries: total,
        avg_recall,
        avg_precision,
        avg_recall_answerable,
        unanswerable_count,
    }
}

// ============================================================================
// 实验指标：Answer Accuracy 与 Abstention Accuracy
// ============================================================================

/// 单次检索的完整实验记录，包含检索结果与核验结果。
#[derive(Debug, Clone)]
pub struct QueryExperiment {
    pub query: String,
    pub answerable: bool,
    /// 检索命中的 passage ID 列表
    pub retrieved_ids: Vec<String>,
    /// 标注的相关 passage ID 列表
    pub relevant_ids: Vec<String>,
    /// 核验状态
    pub evidence_status: EvidenceStatus,
    /// Recall@K
    pub recall: f32,
    /// Precision@K
    pub precision: f32,
    /// 检索耗时（微秒）
    pub latency_us: u64,
    /// 是否发起了检索（Rule Router 可能跳过）
    pub searched: bool,
}

/// 一次实验的汇总指标（对应 Lecture 要求的六个指标）。
#[derive(Debug, Clone)]
pub struct ExperimentMetrics {
    /// 策略名称
    pub strategy_name: String,
    /// Answer Accuracy：可回答 query 中找到支持证据的比例
    pub answer_accuracy: f32,
    /// Recall@3：相关文档中被检索出的比例（仅可回答 query）
    pub recall_at_3: f32,
    /// Citation Correctness：检索出的 ID 与相关 ID 匹配的比例
    pub citation_correctness: f32,
    /// Abstention Accuracy：不可回答 query 中正确拒答的比例
    pub abstention_accuracy: f32,
    /// 平均搜索次数（每 query 发起的检索次数）
    pub avg_searches: f32,
    /// 平均延迟（微秒）
    pub avg_latency_us: f32,
    /// 总 query 数
    pub total_queries: usize,
    /// 可回答 query 数
    pub answerable_count: usize,
    /// 不可回答 query 数
    pub unanswerable_count: usize,
}

/// 计算 Answer Accuracy 和 Abstention Accuracy。
///
/// - **Answer Accuracy**：对于 `answerable=true` 的 query，
///   核验结果为 `Supported`、`Partial` 或 `Conflicting` 视为正确
///   （Conflicting 表示系统正确识别了多源冲突，这本身就是有价值的信息）。
/// - **Abstention Accuracy**：对于 `answerable=false` 的 query，
///   核验结果为 `Insufficient` 视为正确拒答。
/// - **Citation Correctness**：检索到的 ID 中与标注相关 ID 匹配的比例（即 Precision）。
pub fn compute_metrics(
    strategy_name: &str,
    experiments: &[QueryExperiment],
) -> ExperimentMetrics {
    let total = experiments.len();
    let answerable: Vec<&QueryExperiment> =
        experiments.iter().filter(|e| e.answerable).collect();
    let unanswerable: Vec<&QueryExperiment> =
        experiments.iter().filter(|e| !e.answerable).collect();

    // Answer Accuracy：可回答 query 中 evidence 判定为
    // Supported/Partial/Conflicting 的比例。
    // Conflicting 也是正确答案 —— 系统正确识别了多源证据间的差异。
    let answer_accuracy = if answerable.is_empty() {
        0.0
    } else {
        let correct = answerable
            .iter()
            .filter(|e| {
                matches!(
                    e.evidence_status,
                    EvidenceStatus::Supported
                        | EvidenceStatus::Partial
                        | EvidenceStatus::Conflicting
                )
            })
            .count();
        correct as f32 / answerable.len() as f32
    };

    // Recall@3（仅可回答 query）
    let recall_at_3 = if answerable.is_empty() {
        0.0
    } else {
        answerable.iter().map(|e| e.recall).sum::<f32>() / answerable.len() as f32
    };

    // Citation Correctness = Precision（检索结果中真正相关的比例）
    let citation_correctness = if total == 0 {
        0.0
    } else {
        experiments.iter().map(|e| e.precision).sum::<f32>() / total as f32
    };

    // Abstention Accuracy：不可回答 query 中正确拒答的比例
    let abstention_accuracy = if unanswerable.is_empty() {
        1.0 // 没有不可回答 query 时视为满分
    } else {
        let correct = unanswerable
            .iter()
            .filter(|e| e.evidence_status == EvidenceStatus::Insufficient)
            .count();
        correct as f32 / unanswerable.len() as f32
    };

    // 平均搜索次数
    let avg_searches = if total == 0 {
        0.0
    } else {
        experiments
            .iter()
            .filter(|e| e.searched)
            .count() as f32
            / total as f32
    };

    // 平均延迟
    let avg_latency_us = if total == 0 {
        0.0
    } else {
        experiments.iter().map(|e| e.latency_us as f32).sum::<f32>() / total as f32
    };

    ExperimentMetrics {
        strategy_name: strategy_name.to_owned(),
        answer_accuracy,
        recall_at_3,
        citation_correctness,
        abstention_accuracy,
        avg_searches,
        avg_latency_us,
        total_queries: total,
        answerable_count: answerable.len(),
        unanswerable_count: unanswerable.len(),
    }
}

/// 识别三类失败样本。
///
/// 返回 `(检索失败, 推理失败, 引用失败)` 各一个代表性样本。
/// 若无该类型失败则返回 `None`。
pub fn find_failure_samples(
    experiments: &[QueryExperiment],
) -> (Option<&QueryExperiment>, Option<&QueryExperiment>, Option<&QueryExperiment>) {
    // 检索失败：可回答 query 但 Recall=0（啥也没搜到）
    let retrieval_failure = experiments
        .iter()
        .find(|e| e.answerable && e.recall == 0.0);

    // 推理失败：可回答 query 搜到了东西但 verify 判定为 Insufficient
    let reasoning_failure = experiments.iter().find(|e| {
        e.answerable
            && e.recall > 0.0
            && e.evidence_status == EvidenceStatus::Insufficient
    });

    // 引用失败：不可回答 query 却返回了 Supported/Partial（应拒答而未拒答）
    let citation_failure = experiments.iter().find(|e| {
        !e.answerable
            && matches!(
                e.evidence_status,
                EvidenceStatus::Supported | EvidenceStatus::Partial
            )
    });

    (retrieval_failure, reasoning_failure, citation_failure)
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recall_perfect_when_all_found() {
        let r = recall_at_k(
            &["A#1".into(), "B#2".into()],
            &["A#1".into(), "B#2".into()],
        );
        assert_eq!(r, 1.0);
    }

    #[test]
    fn recall_zero_when_none_found() {
        let r = recall_at_k(&["X#99".into()], &["A#1".into()]);
        assert_eq!(r, 0.0);
    }

    #[test]
    fn recall_partial() {
        let r = recall_at_k(
            &["A#1".into(), "X#99".into()],
            &["A#1".into(), "B#2".into()],
        );
        assert_eq!(r, 0.5);
    }

    #[test]
    fn recall_is_one_when_no_relevant_docs() {
        let r = recall_at_k(&["A#1".into()], &[]);
        assert_eq!(r, 1.0);
    }

    #[test]
    fn precision_perfect_when_all_relevant() {
        let p = precision_at_k(
            &["A#1".into()],
            &["A#1".into(), "B#2".into()],
        );
        assert_eq!(p, 1.0);
    }

    #[test]
    fn precision_zero_when_none_relevant() {
        let p = precision_at_k(&["X#99".into()], &["A#1".into()]);
        assert_eq!(p, 0.0);
    }

    #[test]
    fn precision_zero_when_empty_retrieved() {
        let p = precision_at_k(&[], &["A#1".into()]);
        assert_eq!(p, 0.0);
    }

    #[test]
    fn passage_id_format_is_source_locator() {
        let p = Passage {
            source_id: "REG-2024".into(),
            locator: "§3.2".into(),
            text: "...".into(),
            version: "v2".into(),
            effective_date: "2024-01-01".into(),
        };
        assert_eq!(passage_id(&p), "REG-2024#§3.2");
    }

    #[test]
    fn evaluate_queries_computes_recall_for_all() {
        let queries = vec![
            EvalQuery {
                query: "保证金".into(),
                relevant_ids: vec!["REG-2024#§3.2".into()],
                answerable: true,
            },
        ];
        let passages = vec![Passage {
            source_id: "REG-2024".into(),
            locator: "§3.2".into(),
            text: "投标保证金不得超过2%".into(),
            version: "v2".into(),
            effective_date: "2024-01-01".into(),
        }];

        let evals = evaluate_queries(&queries, &passages, 3, crate::search);
        assert_eq!(evals.len(), 1);
        // 检索应能找到 §3.2
        assert!(evals[0].recall > 0.0);
    }

    #[test]
    fn summary_computes_averages_correctly() {
        let evals = vec![
            SearchEval {
                query: "q1".into(),
                retrieved_ids: vec!["A#1".into()],
                relevant_ids: vec!["A#1".into()],
                recall: 1.0,
                precision: 1.0,
                answerable: true,
            },
            SearchEval {
                query: "q2".into(),
                retrieved_ids: vec![],
                relevant_ids: vec!["B#2".into()],
                recall: 0.0,
                precision: 0.0,
                answerable: true,
            },
        ];
        let summary = summarize(&evals);
        assert_eq!(summary.total_queries, 2);
        assert_eq!(summary.avg_recall, 0.5);
        assert_eq!(summary.avg_precision, 0.5);
        assert_eq!(summary.unanswerable_count, 0);
    }
}
