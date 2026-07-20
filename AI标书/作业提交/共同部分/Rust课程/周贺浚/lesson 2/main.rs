
struct Document {
    title: String,
    content: String,
}

fn search(query: &str, documents: &[Document]) -> Vec<Document> {
    let keywords: Vec<&str> = query
        .split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
        .filter(|w| w.chars().count() >= 2)
        .collect();

    let mut scored: Vec<(usize, &Document)> = documents
        .iter()
        .filter_map(|doc| {
            let count = keywords
                .iter()
                .filter(|k| doc.content.contains(**k))
                .count();
            if count > 0 { Some((count, doc)) } else { None }
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0));

    scored
        .iter()
        .map(|&(_, doc)| {
            let snippet: String = doc.content.chars().take(200).collect();
            Document {
                title: doc.title.clone(),
                content: snippet,
            }
        })
        .collect()
}

fn main() {
    let documents = vec![
        Document {
            title: "政府采购法 第22条".into(),
            content: "供应商参加政府采购活动应当具备下列条件：（一）具有独立承担民事责任的能力；（二）具有良好的商业信誉和健全的财务会计制度；（三）具有履行合同所必需的设备和专业技术能力；（四）有依法缴纳税收和社会保障资金的良好记录；（五）参加政府采购活动前三年内，在经营活动中没有重大违法记录；（六）法律、行政法规规定的其他条件。采购人可以根据采购项目的特殊要求，规定供应商的特定条件，但不得以不合理的条件对供应商实行差别待遇或者歧视待遇。".into(),
        },
        Document {
            title: "招标投标法 第20条".into(),
            content: "招标文件不得要求或者标明特定的生产供应者以及含有倾向或者排斥潜在投标人的其他内容。招标人应当根据招标项目的特点和需要编制招标文件。招标文件应当包括招标项目的技术要求、对投标人资格审查的标准、投标报价要求和评标标准等所有实质性要求和条件以及拟签订合同的主要条款。国家对招标项目的技术、标准有规定的，招标人应当按照其规定在招标文件中提出相应要求。".into(),
        },
        Document {
            title: "民法典 合同编 第470条".into(),
            content: "合同的内容由当事人约定，一般包括下列条款：（一）当事人的姓名或者名称和住所；（二）标的；（三）数量；（四）质量；（五）价款或者报酬；（六）履行期限、地点和方式；（七）违约责任；（八）解决争议的方法。当事人可以参照各类合同的示范文本订立合同。当事人应当遵循诚信原则，根据合同的性质、目的和交易习惯履行通知、协助、保密等义务。".into(),
        },
    ];

    for query in &["招标 供应商", "合同 履行", "投标 资格 条件"] {
        println!("=== 搜索：{} ===", query);
        let results = search(query, &documents);
        if results.is_empty() {
            println!("  无匹配结果\n");
        } else {
            for doc in &results {
                let preview: String = doc.content.chars().take(50).collect();
            println!("  ▶ {} | snippet: {}...", doc.title, preview);
            }
            println!();
        }
    }
}
