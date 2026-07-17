#[derive(Debug, Clone)]
struct Document {
    title: String,
    content: String,
}

impl Document {
    fn get_snippet(&self, keywords: &[String]) -> String {
        let lines = self.content.lines();
        for line in lines {
            let lower_line = line.to_lowercase();
            for kw in keywords {
                if lower_line.contains(kw) {
                    return line.to_string();
                }
            }
        }
        self.content.chars().take(50).collect()
    }
}

fn split_keywords(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|s| s.to_lowercase())
        .filter(|word| word.len() >= 2)
        .collect()
}

fn search(query: &str, documents: &[Document]) -> Vec<Document> {
    let keywords = split_keywords(query);
    if keywords.is_empty() {
        return Vec::new();
    }

    let mut doc_with_hit_count: Vec<(usize, Document)> = documents
        .iter()
        .map(|doc| {
            let doc_lower = doc.content.to_lowercase();
            let hit_num = keywords.iter().filter(|kw| doc_lower.contains(*kw)).count();
            (hit_num, doc.clone())
        })
        .filter(|&(count, _)| count > 0)
        .collect();
    doc_with_hit_count.sort_by(|a, b| b.0.cmp(&a.0));
    doc_with_hit_count.into_iter().map(|(_, doc)| doc).collect()
}

fn main() {
    let docs = vec![
        Document {
            title: "政府采购法 第22条".to_string(),
            content: "供应商应当具有独立承担民事责任的能力，良好商业信誉，健全财务会计制度。参与政府采购不得提供虚假材料，禁止围标串标行为。".to_string(),
        },
        Document {
            title: "招标投标法 第20条".to_string(),
            content: "招标文件不得要求或者标明特定的生产供应者，不得含有倾向或者排斥潜在投标人的内容，公平对待所有竞标企业。".to_string(),
        },
        Document {
            title: "招标投标法实施条例 第34条".to_string(),
            content: "单位负责人为同一人或者存在控股管理关系的不同单位，不得同时参加同一标段投标，否则相关投标均无效，视为围标。".to_string(),
        },
    ];
    println!("===== 搜索关键词：围标 =====");
    let res1 = search("围标", &docs);
    for d in res1 {
        println!("标题：{}", d.title);
        println!("片段：{}\n", d.get_snippet(&split_keywords("围标")));
    }

    println!("===== 搜索关键词：投标 供应商 =====");
    let res2 = search("投标 供应商", &docs);
    for d in res2 {
        println!("标题：{}", d.title);
        println!("片段：{}\n", d.get_snippet(&split_keywords("投标 供应商")));
    }
    println!("===== 搜索关键词：一 二 =====");
    let res3 = search("一 二", &docs);
    if res3.is_empty() {
        println!("无匹配文档\n");
    }
}
