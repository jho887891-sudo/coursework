#[derive(Debug, Clone)]
struct Document {
    title: String,
    content: String,
}

fn search(query: &str, documents: &[Document]) -> Vec<Document> {
    let keywords: Vec<&str> = query
        .split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
        .filter(|word| word.chars().count() >= 2)
        .collect();

    if keywords.is_empty() {
        return Vec::new();
    }

    let mut results: Vec<(usize, Document)> = documents
        .iter()
        .filter_map(|doc| {
            let hit_count = keywords
                .iter()
                .filter(|&&kw| doc.content.contains(kw) || doc.title.contains(kw))
                .count();
            if hit_count > 0 {
                Some((hit_count, doc.clone()))
            } else {
                None
            }
        })
        .collect();

    results.sort_by(|a, b| b.0.cmp(&a.0));

    results.into_iter().map(|(_, doc)| doc).collect()
}

fn main() {
    let documents = vec![
        Document {
            title: String::from("政府采购法 第22条"),
            content: String::from("采购人或者采购代理机构有下列情形之一的，属于以不合理条件对供应商实行差别待遇或者歧视待遇：（一）就同一采购项目向供应商提供有差别的项目信息；（二）设定的资格、技术、商务条件与采购项目的具体特点和实际需要不相适应或者与合同履行无关；"),
        },
        Document {
            title: String::from("招标投标法 第20条"),
            content: String::from("招标文件不得要求或者标明特定的生产供应者以及含有倾向或者排斥潜在投标人的其他内容。"),
        },
        Document {
            title: String::from("招标投标法实施条例 第34条"),
            content: String::from("与招标人存在利害关系可能影响招标公正性的法人、其他组织或者个人，不得参加投标。单位负责人为同一人或者存在控股、管理关系的不同单位，不得参加同一标段投标或者未划分标段的同一招标项目投标。"),
        },
    ];

    let queries = vec!["供应商", "招标", "投标", "采购"];

    for query in queries {
        println!("\n搜索: {}", query);
        let results = search(query, &documents);
        if results.is_empty() {
            println!("无匹配结果");
        } else {
            for (i, doc) in results.iter().enumerate() {
                println!("  {}. {}", i + 1, doc.title);
            }
        }
    }
}
