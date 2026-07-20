/// 简单文本搜索引擎

#[derive(Debug, Clone)]
struct Document {
    title: String,
    content: String,
}

/// 从正文中截取包含关键词的片段（以字符为单位）
fn extract_snippet(content: &str, keywords: &[&str], char_radius: usize) -> String {
    // 找到第一个命中的关键词位置（字节索引）
    let mut best_byte = None;
    let mut best_kw_len = 0;
    for kw in keywords {
        if let Some(pos) = content.find(kw) {
            best_byte = Some(pos);
            best_kw_len = kw.chars().count();
            break;
        }
    }

    match best_byte {
        None => {
            let snippet: String = content.chars().take(80).collect();
            if content.chars().count() > 80 {
                snippet + "…"
            } else {
                snippet
            }
        }
        Some(byte_pos) => {
            // 找到命中位置对应的字符索引
            let hit_char_idx = content[..byte_pos].chars().count();

            let total_chars = content.chars().count();
            let start_char = hit_char_idx.saturating_sub(char_radius);
            let end_char = (hit_char_idx + best_kw_len + char_radius).min(total_chars);

            // 用字符迭代器截取，避免 UTF-8 边界问题
            let snippet: String = content
                .chars()
                .skip(start_char)
                .take(end_char - start_char)
                .collect();

            let prefix = if start_char > 0 { "…" } else { "" };
            let suffix = if end_char < total_chars { "…" } else { "" };
            format!("{}{}{}", prefix, snippet, suffix)
        }
    }
}

/// 搜索：拆词 → 过滤短词 → 命中 → 按命中数降序
fn search(query: &str, documents: &[Document]) -> Vec<Document> {
    // 拆词 + 去除非字母数字 + 去掉长度 < 2
    let keywords: Vec<&str> = query
        .split(|c: char| !c.is_alphanumeric() && c != '-') // 按标点/空白切分
        .map(|w| w.trim())
        .filter(|w| w.len() >= 2)
        .collect();

    if keywords.is_empty() {
        return Vec::new();
    }

    // 计算每个文档的命中数和带 snippet 的文档
    let mut results: Vec<(usize, Document)> = documents
        .iter()
        .filter_map(|doc| {
            // 检查内容中命中哪些关键词
            let hits: Vec<&&str> = keywords
                .iter()
                .filter(|kw| doc.content.contains(**kw))
                .collect();

            if hits.is_empty() {
                return None;
            }

            let snippet_keywords: Vec<&str> =
                hits.iter().map(|s| **s).collect();
            let snippet = extract_snippet(&doc.content, &snippet_keywords, 40);

            Some((
                hits.len(),
                Document {
                    title: doc.title.clone(),
                    content: snippet,
                },
            ))
        })
        .collect();

    // 按命中数降序排列（稳定排序，命中数相同则保持原顺序）
    results.sort_by(|a, b| b.0.cmp(&a.0));

    results.into_iter().map(|(_, doc)| doc).collect()
}

fn main() {
    let documents = vec![
        Document {
            title: "政府采购法 第22条".into(),
            content: "供应商参加政府采购活动应当具备下列条件：（一）具有独立承担民事责任的能力；（二）具有良好的商业信誉和健全的财务会计制度；（三）具有履行合同所必需的设备和专业技术能力；（四）有依法缴纳税收和社会保障资金的良好记录；（五）参加政府采购活动前三年内，在经营活动中没有重大违法记录；（六）法律、行政法规规定的其他条件。采购人可以根据采购项目的特殊要求，规定供应商的特定条件，但不得以不合理的条件对供应商实行差别待遇或者歧视待遇。".into(),
        },
        Document {
            title: "招标投标法 第20条".into(),
            content: "招标文件不得要求或者标明特定的生产供应者以及含有倾向或者排斥潜在投标人的其他内容。招标人不得以不合理的条件限制或者排斥潜在投标人，不得对潜在投标人实行歧视待遇。招标文件应当包括招标项目的技术要求、对投标人资格审查的标准、投标报价要求和评标标准等所有实质性要求和条件以及拟签订合同的主要条款。".into(),
        },
        Document {
            title: "政府采购法 第23条".into(),
            content: "采购人可以要求参加政府采购的供应商提供有关资质证明文件和业绩情况，并根据本法规定的供应商条件和采购项目对供应商的特定要求，对供应商的资格进行审查。采购人应当根据采购项目的特点和实际需求，制定科学合理的采购方案，确保采购活动的公开、公平、公正。".into(),
        },
        Document {
            title: "民法典 合同编 第470条".into(),
            content: "合同的内容由当事人约定，一般包括下列条款：（一）当事人的姓名或者名称和住所；（二）标的；（三）数量；（四）质量；（五）价款或者报酬；（六）履行期限、地点和方式；（七）违约责任；（八）解决争议的方法。当事人可以参照各类合同的示范文本订立合同。依法成立的合同，受法律保护。".into(),
        },
    ];

    let test_queries = vec!["供应商 条件", "歧视待遇", "合同 条款", "不存在的词"];

    for query in test_queries {
        println!("搜索: \"{}\"", query);
        println!("{}", "-".repeat(40));

        let results = search(query, &documents);

        if results.is_empty() {
            println!("  无匹配结果\n");
        } else {
            for (i, doc) in results.iter().enumerate() {
                println!("  {}. {}", i + 1, doc.title);
                println!("     {}", doc.content);
                println!();
            }
        }
    }
}
