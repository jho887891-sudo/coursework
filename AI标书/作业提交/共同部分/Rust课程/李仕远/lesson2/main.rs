#[derive(Debug, Clone)]
struct Document{
    title:String,
    content:String,
}

#[derive(Debug)]
struct SearchResult {
    title: String,
    snippet: String,
    score: u32,
}

fn search(query:&str,documnets:&[Document]) ->Vec<SearchResult>{
    //处理query关键词
    let keywords :Vec<&str> = query.split_whitespace().filter(|w| w.len()>=2).collect();
    if keywords.is_empty() {
        return vec![];
    }

    let mut results : Vec<SearchResult> = Vec::new();

    for doc in documnets {
        let mut score = 0;
        //计分
        for keyword in &keywords {
            if doc.content.contains(keyword){
                score +=1;
            }
        }
        if score > 0 {
            let res = SearchResult {
                title : doc.title.clone(),
                snippet:doc.content.clone(),
                score: score,
            };
            results.push(res);
        }
    }

    results.sort_by(|a,b| b.score.cmp(&a.score));
    return results;
}



fn main() {
    let docs = vec![
        Document {
            title: "政府采购法 第22条".to_string(),
            content: "供应商参加政府采购活动应当具备下列条件：具有独立承担民事责任的能力；具有良好的商业信誉和健全的财务会计制度...".to_string(),
        },
        Document {
            title: "招标投标法 第20条".to_string(),
            content: "招标文件不得要求或者标明特定的生产供应者以及含有倾向或者排斥潜在投标人的其他内容。".to_string(),
        },
        Document {
            title: "合同法 第10条".to_string(),
            content: "当事人订立合同，有书面形式、口头形式和其他形式。法律、行政法规规定采用书面形式的，应当采用书面形式。".to_string(),
        },
    ];

    //测试
    let res1 = search("采购", &docs);
    for r in res1 {
        println!("[res1]标题: {}，片段：{}，分数：{}", r.title,r.snippet,r.score);
    }

    let res2 = search("形式 招标", &docs);
    for r in res2 {
        println!("[res2]标题: {}，片段：{}，分数：{}", r.title,r.snippet,r.score);
    }
}
