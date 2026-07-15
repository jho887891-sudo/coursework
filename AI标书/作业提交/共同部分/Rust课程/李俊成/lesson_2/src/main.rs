
#[derive(Debug, Clone)]
struct Document {
    title: String,
    content: String,
}

// 实现搜索函数
fn search(query: &str, documents: &[Document]) -> Vec<Document> {
    // 搜索逻辑：query 拆词 ,去掉长度 < 2 的词
    let keywords: Vec<&str> = query
        .split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
        .filter(|word| word.chars().count() >= 2)
        .collect();

    if keywords.is_empty() {
        return Vec::new();
    }

    // 找到命中任意关键词的文档
    let mut matched_docs: Vec<Document> = documents
        .iter()
        .filter(|doc| {
            keywords.iter().any(|kw| doc.content.contains(kw))
        })
        .cloned() 
        .collect();

    // 按命中关键词数量降序排列
    matched_docs.sort_by(|a, b| {
        let count_a = keywords.iter().filter(|kw| a.content.contains(*kw)).count();
        let count_b = keywords.iter().filter(|kw| b.content.contains(*kw)).count();
        
        count_b.cmp(&count_a)
    });

    matched_docs
}

fn main() {
    // 硬编码 3 段文本
    let doc1 = Document {
        title: "政府采购法 第22条".into(),
        content: "供应商参加政府采购活动应当具备下列条件：具有独立承担民事责任的能力。".into(),
    };

    let doc2 = Document {
        title: "招标投标法 第20条".into(),
        content: "招标文件不得要求或者标明特定的生产供应者以及含有倾向或者排斥潜在投标人的其他内容。".into(),
    };

    let doc3 = Document {
        title: "AI Agent 项目研发规范".into(),
        content: "研发 AI Agent 系统的供应商需要具备大模型集成开发能力。在政府采购和项目招投标过程中，需要严格遵守相关法律。".into(),
    };

    let corpus = vec![doc1, doc2, doc3];

    // 测试搜索词
    let query = "供应商 政府采购法";
    println!("正在搜索关键词: \"{}\"", query);

    let results = search(query, &corpus);

    //打印匹配的文档标题
    for (idx, doc) in results.iter().enumerate() {
        println!("排名 {}: {}", idx + 1, doc.title);
    }
}