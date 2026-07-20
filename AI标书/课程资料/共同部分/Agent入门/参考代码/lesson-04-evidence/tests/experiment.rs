//! # Lesson 4 实验：三种检索策略对比
//!
//! 在同一评测集（20 条 query、10 条 passage）上比较：
//!
//! | 策略 | 说明 |
//! |---|---|
//! | AlwaysRetrieve | 每个 query 都检索 Top-3 |
//! | RuleRouter     | 命中规则关键词才检索 |
//! | AgenticRetrieve| 模型决策（当前 baseline 回退为 Always） |
//!
//! ## 六个指标
//!
//! 1. Answer Accuracy      — 可回答 query 中找到支持证据的比例
//! 2. Recall@3             — 相关文档中被检索出的比例
//! 3. Citation Correctness — 检索结果真正相关的比例（Precision）
//! 4. Abstention Accuracy  — 不可回答 query 中正确拒答的比例
//! 5. Average Searches     — 每 query 平均检索次数
//! 6. Average Latency      — 每 query 平均耗时（μs）
//!
//! ## 三个失败样本
//!
//! - 检索失败：可回答但 Recall=0
//! - 推理失败：搜到了但 verify 判 Insufficient
//! - 引用失败：不可回答却未拒答
//!
//! 运行方式：
//! ```powershell
//! cargo test -p lesson-04-evidence --test experiment -- --nocapture
//! ```

use lesson_04_evidence::*;
use std::path::PathBuf;
use std::time::Instant;

// ============================================================================
// 数据路径
// ============================================================================

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("公开数据")
        .join("lesson-04-evidence")
}

// ============================================================================
// 单次实验执行
// ============================================================================

/// 对一条 query 执行完整的 检索→核验 流程。
///
/// 返回包含所有指标的 `QueryExperiment` 记录。
fn run_single_query(
    query: &EvalQuery,
    passages: &[Passage],
    k: usize,
    strategy: RetrievalStrategy,
) -> QueryExperiment {
    let start = Instant::now();

    // 1. 策略决策：是否检索？
    let decision = decide_retrieval(strategy, &query.query, &[]);
    let (searched, hits) = match &decision {
        RetrieveDecision::Search { query: rewritten } => {
            // 2. 检索
            let result = search(rewritten, passages, k);
            (true, result)
        }
        RetrieveDecision::Skip { .. } => {
            // 不检索
            (false, vec![])
        }
    };

    let latency_us = start.elapsed().as_micros() as u64;

    // 3. 提取检索 ID
    let retrieved_ids: Vec<String> = hits.iter().map(|p| passage_id(p)).collect();

    // 4. Recall & Precision
    let recall = recall_at_k(&retrieved_ids, &query.relevant_ids);
    let precision = precision_at_k(&retrieved_ids, &query.relevant_ids);

    // 5. 证据核验：用 query 文本作为 claim，检查是否被检索结果支持
    let evidence_answer = verify(&query.query, &hits);
    let evidence_status = evidence_answer.status;

    QueryExperiment {
        query: query.query.clone(),
        answerable: query.answerable,
        retrieved_ids,
        relevant_ids: query.relevant_ids.clone(),
        evidence_status,
        recall,
        precision,
        latency_us,
        searched,
    }
}

// ============================================================================
// 完整实验
// ============================================================================

/// 对全部 query 运行一种策略，返回逐条记录。
fn run_strategy(
    queries: &[EvalQuery],
    passages: &[Passage],
    k: usize,
    strategy: RetrievalStrategy,
) -> Vec<QueryExperiment> {
    queries
        .iter()
        .map(|q| run_single_query(q, passages, k, strategy))
        .collect()
}

// ============================================================================
// 输出辅助
// ============================================================================

/// 打印一条分隔线。
fn separator() {
    println!("{:=^72}", "");
}

/// 打印指标表头。
fn print_table_header() {
    println!(
        "{:<20} {:>8} {:>8} {:>10} {:>10} {:>8} {:>10}",
        "策略", "AnsAcc", "Rec@3", "CiteCorr", "AbstAcc", "搜索", "延迟(μs)"
    );
    println!("{:-<72}", "");
}

/// 打印一行指标。
fn print_metrics(m: &ExperimentMetrics) {
    println!(
        "{:<20} {:>7.2}% {:>7.2}% {:>9.2}% {:>9.2}% {:>7.2} {:>9.0}",
        m.strategy_name,
        m.answer_accuracy * 100.0,
        m.recall_at_3 * 100.0,
        m.citation_correctness * 100.0,
        m.abstention_accuracy * 100.0,
        m.avg_searches,
        m.avg_latency_us,
    );
}

/// 打印失败样本分析。
fn print_failure_sample(label: &str, sample: Option<&QueryExperiment>) {
    println!();
    println!("--- {} ---", label);
    match sample {
        None => println!("  ✓ 未出现此类失败"),
        Some(e) => {
            println!("  Query:       {}", e.query);
            println!("  Answerable:  {}", e.answerable);
            println!("  Searched:    {}", e.searched);
            println!("  Recall:      {:.2}", e.recall);
            println!("  Precision:   {:.2}", e.precision);
            println!("  Evidence:    {:?}", e.evidence_status);
            println!("  Retrieved:   {:?}", e.retrieved_ids);
            println!("  Relevant:    {:?}", e.relevant_ids);
        }
    }
}

// ============================================================================
// 主实验测试
// ============================================================================

#[test]
fn compare_three_retrieval_strategies() {
    separator();
    println!("  Lesson 4 实验：三种检索策略对比");
    separator();

    // ---- 加载数据 ----
    let passages = load_passages(&data_dir().join("passages.jsonl"))
        .expect("无法加载 passages.jsonl");
    let queries = load_eval_queries(&data_dir().join("eval_queries.jsonl"))
        .expect("无法加载 eval_queries.jsonl");

    println!();
    println!("  评测集: {} queries ({} 可回答, {} 不可回答), {} passages",
        queries.len(),
        queries.iter().filter(|q| q.answerable).count(),
        queries.iter().filter(|q| !q.answerable).count(),
        passages.len(),
    );

    let k = 3; // Top-K

    // ---- 运行三种策略 ----
    println!();
    println!("  运行中...");

    let always_results = run_strategy(&queries, &passages, k, RetrievalStrategy::AlwaysRetrieve);
    let rule_results = run_strategy(&queries, &passages, k, RetrievalStrategy::RuleRouter);
    let agentic_results = run_strategy(&queries, &passages, k, RetrievalStrategy::AgenticRetrieve);

    // ---- 计算指标 ----
    let always_metrics = compute_metrics("AlwaysRetrieve", &always_results);
    let rule_metrics = compute_metrics("RuleRouter", &rule_results);
    let agentic_metrics = compute_metrics("AgenticRetrieve", &agentic_results);

    // ---- 输出对比表 ----
    println!();
    println!("  ◎ 六指标对比表");
    println!();
    print_table_header();
    print_metrics(&always_metrics);
    print_metrics(&rule_metrics);
    print_metrics(&agentic_metrics);
    println!();

    // ---- 策略差异解读 ----
    println!("  ◎ 策略差异解读");
    println!();
    println!("  AlwaysRetrieve:  每个 query 都检索，搜索次数=1.0，Recall 最高。");
    println!("  RuleRouter:      仅命中触发词时检索，搜索次数<1.0，但可能漏掉");
    println!("                   不含显式关键词的问题（Recall 下降）。");
    println!(
        "  AgenticRetrieve: 当前 baseline 回退为 Always，结果与 Always 相同。"
    );
    println!("                   需要真实模型参与决策才能体现差异。");

    // ---- 失败样本分析 ----
    println!();
    println!("  ◎ 失败样本分析（以 AlwaysRetrieve 为例）");
    let (ret_fail, reas_fail, cit_fail) = find_failure_samples(&always_results);

    print_failure_sample("检索失败：可回答但 Recall=0（关键词不匹配）", ret_fail);

    print_failure_sample(
        "推理失败：搜到了但 verify 判 Insufficient（主题相关≠逻辑支持）",
        reas_fail,
    );

    print_failure_sample(
        "引用失败：不可回答却返回了证据（应拒答而未拒答）",
        cit_fail,
    );

    println!();
    println!("  ◎ RuleRouter 漏检分析");
    let rule_skipped: Vec<_> = rule_results.iter().filter(|e| !e.searched).collect();
    if rule_skipped.is_empty() {
        println!("  RuleRouter 未跳过任何 query（所有 query 都包含触发词）。");
    } else {
        println!("  RuleRouter 跳过了 {}/{} 条 query：", rule_skipped.len(), queries.len());
        for e in &rule_skipped {
            let should_have_searched = e.answerable && !e.relevant_ids.is_empty();
            let marker = if should_have_searched { " ⚠ 漏检!" } else { "" };
            println!("    - \"{}\"{}", e.query, marker);
        }
    }

    // ---- 严肃断言：验证实验不变量 ----
    // 这些断言确保实验没有静默失败

    // 1. 评测数据完整性
    assert_eq!(queries.len(), 20, "评测集应有 20 条 query");
    assert!(!passages.is_empty(), "语料不应为空");

    // 2. Always 必须每条都检索
    assert!(
        always_results.iter().all(|e| e.searched),
        "AlwaysRetrieve 必须每条都检索"
    );

    // 3. Agentic baseline 与 Always 结果一致（当前回退实现）
    for (a, ag) in always_results.iter().zip(agentic_results.iter()) {
        assert_eq!(a.recall, ag.recall,
            "Agentic baseline 应与 Always 一致: query='{}'", a.query);
    }

    // 4. RuleRouter 搜索次数 ≤ Always
    let rule_searches: usize = rule_results.iter().filter(|e| e.searched).count();
    let always_searches: usize = always_results.iter().filter(|e| e.searched).count();
    assert!(
        rule_searches <= always_searches,
        "RuleRouter 搜索次数({})不应超过 Always({})",
        rule_searches,
        always_searches,
    );

    // 5. 不可回答 query 的 Recall 应为 0（无相关文档可检索）
    for e in &always_results {
        if !e.answerable {
            assert!(
                e.relevant_ids.is_empty(),
                "标注矛盾: query='{}' answerable=false 但 relevant_ids 非空", e.query
            );
        }
    }

    println!();
    separator();
    println!("  实验完成 — 所有不变量验证通过");
    separator();
}
