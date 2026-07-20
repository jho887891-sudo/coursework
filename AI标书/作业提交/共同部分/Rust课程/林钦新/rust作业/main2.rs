// ============================================================
// 简易文本搜索引擎 — 基于关键词匹配 + 排序
// ============================================================

/// 文档：标题 + 正文
#[derive(Debug, Clone)]
struct Document {
    title: String,
    content: String,
}

/// 在 content 中提取第一个命中关键词附近的片段（摘要）
fn extract_snippet(content: &str, keywords: &[String]) -> String {
    // 找到最早出现的关键词的字节位置
    let mut best_byte: Option<usize> = None;
    for kw in keywords {
        if let Some(pos) = content.find(kw.as_str()) {
            match best_byte {
                None => best_byte = Some(pos),
                Some(b) if pos < b => best_byte = Some(pos),
                _ => {}
            }
        }
    }

    let chars: Vec<char> = content.chars().collect();
    let total = chars.len();
    let window = 35; // 命中位置前后各取 35 个字符

    match best_byte {
        None => {
            // 理论上不会到这里（调用方已保证有关键词命中）
            chars.iter().take(100).collect()
        }
        Some(byte_pos) => {
            let char_idx = content[..byte_pos].chars().count(); // 字节 → 字符下标
            let start = char_idx.saturating_sub(window);        // 防止 <0
            let end = (char_idx + window + 10).min(total);

            let mut snippet = String::new();
            if start > 0 {
                snippet.push_str("…");
            }
            snippet.push_str(&chars[start..end].iter().collect::<String>());
            if end < total {
                snippet.push_str("…");
            }
            snippet
        }
    }
}

/// 核心搜索函数
///
/// 流程：
/// 1. query 拆词（按非字母数字字符分割）
/// 2. 去掉长度 < 2 的短词
/// 3. 统计每个文档命中了多少个不同关键词
/// 4. 按命中关键词数量降序排列
/// 5. 返回克隆的 Document（title 完整保留，content 替换为摘要片段）
fn search(query: &str, documents: &[Document]) -> Vec<Document> {
    // 1 & 2: 拆词 + 过滤短词
    let keywords: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .map(|w| w.trim().to_lowercase())
        .filter(|w| w.chars().count() >= 2)
        .collect();

    if keywords.is_empty() {
        return vec![];
    }

    // 3: 统计每个文档命中的不同关键词数量
    let mut scored: Vec<(usize, usize, &Document)> = documents
        .iter()
        .enumerate()
        .filter_map(|(idx, doc)| {
            let content_lower = doc.content.to_lowercase();
            let hits: usize = keywords
                .iter()
                .filter(|kw| content_lower.contains(kw.as_str()))
                .count();
            if hits > 0 {
                Some((hits, idx, doc))
            } else {
                None
            }
        })
        .collect();

    // 4: 按命中数降序（命中多的排前面）
    scored.sort_by(|a, b| b.0.cmp(&a.0));

    // 5: 组装结果 — title 克隆，content 替换为摘要
    scored
        .into_iter()
        .map(|(_hits, _idx, doc)| Document {
            title: doc.title.clone(),
            content: extract_snippet(&doc.content, &keywords),
        })
        .collect()
}

// ============================================================
// main — 测试
// ============================================================
fn main() {
    let documents = vec![
        Document {
            title: "政府采购法 第22条".into(),
            content: "供应商参加政府采购活动应当具备下列条件：（一）具有独立承担民事责任的能力；\
                      （二）具有良好的商业信誉和健全的财务会计制度；（三）具有履行合同所必需的设备\
                      和专业技术能力；（四）有依法缴纳税收和社会保障资金的良好记录；（五）参加政府\
                      采购活动前三年内，在经营活动中没有重大违法记录；（六）法律、行政法规规定的其他\
                      条件。采购人可以根据采购项目的特殊要求，规定供应商的特定条件，但不得以不合理的\
                      条件对供应商实行差别待遇或者歧视待遇。"
                .into(),
        },
        Document {
            title: "招标投标法 第20条".into(),
            content: "招标文件不得要求或者标明特定的生产供应者以及含有倾向或者排斥潜在投标人的其他\
                      内容。招标人不得以不合理的条件限制或者排斥潜在投标人，不得对潜在投标人实行歧视\
                      待遇。招标文件应当包括招标项目的技术要求、对投标人资格审查的标准、投标报价要求\
                      和评标标准等所有实质性要求和条件以及拟签订合同的主要条款。国家对招标项目的技术、\
                      标准等有规定的，招标人应当按照其规定在招标文件中提出相应要求。"
                .into(),
        },
        Document {
            title: "民法典 第470条".into(),
            content: "合同的内容由当事人约定，一般包括下列条款：（一）当事人的姓名或者名称和住所；\
                      （二）标的；（三）数量；（四）质量；（五）价款或者报酬；（六）履行期限、地点\
                      和方式；（七）违约责任；（八）解决争议的方法。当事人可以参照各类合同的示范文本\
                      订立合同。依法成立的合同，受法律保护。当事人应当按照约定全面履行自己的义务，\
                      遵循诚信原则，根据合同的性质、目的和交易习惯履行通知、协助、保密等义务。"
                .into(),
        },
    ];

    // ---- 测试搜索 ----
    let queries = vec![
        "供应商 条件",
        "合同 履行 义务",
        "招标 投标人 歧视",
        "采购",
    ];

    for q in queries {
        println!("═══════════════════════════════════════");
        println!("🔍 搜索: \"{}\"", q);
        println!("───────────────────────────────────────");

        let results = search(q, &documents);

        if results.is_empty() {
            println!("  (无匹配结果)");
        } else {
            for (i, doc) in results.iter().enumerate() {
                println!("  {}. {}", i + 1, doc.title);
                println!("     …{}", doc.content);
                println!();
            }
        }
    }
}
