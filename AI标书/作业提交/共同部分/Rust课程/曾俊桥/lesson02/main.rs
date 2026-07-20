#[derive(Debug,Clone)]
struct Document{
    title:String,
    content:String
}

fn search(query:&str,documents:&[Document])->Vec<Document> {
    let keywords: Vec<String> = query.to_lowercase()
        .split_whitespace()
        .filter(|word| word.len() >= 2)
        .map(|s| s.to_string())
        .collect();

    let mut doc_with_score: Vec<(Document, usize)> = documents.iter()
        .map(|doc| {
            let doc_content = doc.content.to_lowercase();
            let hit_count = keywords.iter()
                .filter(|kw| doc_content.contains(*kw))
                .count();
            (doc.clone(), hit_count)
        })
        .filter(|(_,hit_count)| *hit_count > 0)
        .collect();
    doc_with_score.sort_by(|a, b| b.1.cmp(&a.1));
    doc_with_score.into_iter().map(|(doc, _hit_count)| doc).collect()
}
fn main(){
    let docs=vec![
        Document{
            title:"政府采购法 第22条".to_string(),
            content:"供应商参加政府采购活动应当具备下列条件：具有独立承担民事责任的能力；具有良好的商业信誉和健全的财务会计制度；具有履行合同所必需的设备和专业技术能力。".to_string(),
        },
        Document{
            title:"招标投资法 第20条".to_string(),
            content:"招标人不得以不合理条件限制或者排斥潜在投标人，不得对潜在投标人实行歧视待遇，保障公平竞争。".to_string(),
        },
        Document{
            title:"政府采购法 第23条".to_string(),
            content:"采购人应当在招标文件中载明对供应商的资格要求，不得以不合理条件限制或者排斥潜在投标人。".to_string(),
        },
    ];

    let test_queries = ["政府采购", "投标人", "合同"];
    for q in test_queries {
        println!("=== 搜索关键词：{} ===", q);
        let result = search(q, &docs);
        if result.is_empty() {
            println!("无匹配文档\n");
        } else {
            for doc in result {
                println!("匹配文档标题：{}", doc.title);
            }
            println!();
        }
    }
}
