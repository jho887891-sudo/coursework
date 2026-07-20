// 课程问题：我们有 3 篇法规文档。用户输入 "政府采购 资格条件"，程序要找出哪些文档匹配，按匹配程度排序，返回结果。
// 不用数据库，不用索引，就用 Rust 的标准库，不到 40 行核心代码。怎么做？
// 思路：把这件事拆成独立的步骤，每一步只做一件事，数据从前一步流到后一步。这就是"数据流水线"。

use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
struct Document {
    title: String,
    content: String,
}

// "政府采购 资格条件"                ← query: &str
//   │ split()                       拆词
//   ▼
// ["政府采购", "资格条件"]
//   │ filter(word.chars().count() >= 2)   去掉短词
//   ▼
// ["政府采购", "资格条件"]
//   │ map(str::to_lowercase)             统一大小写
//   ▼
// ["政府采购", "资格条件"]
//   │ collect::<HashSet>()               去重
//   ▼
// {"政府采购", "资格条件"}          ← keywords: HashSet<String>
//   │ documents.iter().filter_map(...)   遍历文档 + 计分
//   ▼
// [(score:2, 实施条例), (score:1, 第22条)]
//   │ sort_by_key(Reverse(score))        高分在前
//   ▼
// [(score:2, 实施条例), (score:1, 第22条)]
//   │ map(|(_, doc)| doc)                丢掉分数
//   ▼
// [实施条例, 第22条]               ← Vec<Document>


fn search(query: &str, documents: &[Document]) -> Vec<Document> {
    //第一段流水线：查询文本 → 关键词集合
    let keywords: HashSet<String> = query
        .split(|c: char| c.is_whitespace() || c.is_ascii_punctuation()) //按标点符号切割
        .filter(|word| word.chars().count() >= 2) //去掉短词
        .map(str::to_lowercase) //统一大小写
        .collect(); //收集回为HashSet

    //     为什么是 HashSet 而不是 Vec？

    // 假设用户输入 "采购 采购 采购"。如果装在 Vec 里，关键词列表就是 ["采购", "采购", "采购"]。后面计分时，每个 "采购" 都会去文档里匹配一次，一篇文章出现一次"采购"，得分就是 3 而不是 1。用户的重复输入污染了评分。
    // HashSet 自动去重：{"采购"}。一篇文章出现一次"采购"，得分就是 1。
    // Tips：搜索分数反映的是文档和查询之间的匹配程度，而不是用户打了多少遍同一个词。

    //第二段流水线：文档集 → 排序结果
    let mut matches: Vec<(usize, Document)> = documents
        .iter() //借遍历文档
        .filter_map(|document| {
            //(a)构造可搜索文本
            let searchable = format!("{} {}", document.title, document.content).to_lowercase();
            //（b）计算匹配分数
            let score = keywords
                .iter() //借遍历关键词
                .filter(|keyword| searchable.contains(keyword.as_str())) //文本是否包含关键词
                .count();  //统计数量
            //这个就是TF（词频）的简化版，面试会问：为什么不算IDF（逆文档频率）？因为我们只有 3 篇文档，IDF 没有意义。TF-IDF 是在大规模语料库里才有用。同学们可以自行去了解IDF算法。
            //这个方法在后续项目开发中也会经常用到

            // （c）决定返回什么
            (score > 0).then(|| {
                (
                    score,
                    Document {
                        title: document.title.clone(),
                        content: make_snippet(&document.content, &keywords),
                    },
                )
            })
        })
        .collect();

    // sort_by_key 默认升序；Reverse 明确表达"高分在前"。
    matches.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    matches.into_iter().map(|(_, document)| document).collect() //丢掉分数只保留文档本身
}

//生成摘要
fn make_snippet(content: &str, keywords: &HashSet<String>) -> String {
    content
        .split("\n\n")  //按段落分割
        .find(|paragraph| {  //找第一个包含任意关键词的段落
            let paragraph = paragraph.to_lowercase();
            keywords
                .iter()
                .any(|keyword| paragraph.contains(keyword.as_str()))  //：关键词集合中，只要有一个出现在段落里，就返回 true
        })
        .unwrap_or(content)  //如果没找到任何匹配段落，退回全文
        .chars()
        .take(80)
        .collect() //取前 80 个字符，装成 String
}

fn main() {
    let documents = vec![
        Document {
            title: "政府采购法 第22条".into(),
            content: "供应商参加政府采购活动，应当具有独立承担民事责任的能力，并具有良好的商业信誉。".into(),
        },
        Document {
            title: "招标投标法 第20条".into(),
            content: "招标文件不得要求或者标明特定的生产供应者，不得含有倾向或者排斥潜在投标人的内容。".into(),
        },
        Document {
            title: "政府采购法实施条例".into(),
            content: "采购人或者采购代理机构应当根据采购项目的特点编制采购文件。\n\n资格条件不得对供应商实行差别待遇。".into(),
        },
    ];

    for result in search("政府采购 资格条件", &documents) {
        println!("{}\n  {}", result.title, result.content);
    }
}


//测试重复输入同一个关键词，评分会不会虚高
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_keywords_do_not_inflate_the_score() {
        let documents = vec![
            Document {
                title: "A".into(),
                content: "采购".into(),
            },
            Document {
                title: "B".into(),
                content: "采购 资格".into(),
            },
        ];

        let results = search("采购 采购 资格", &documents);
        assert_eq!(results[0].title, "B");
    }
}


//OK，那这个例子除了教会大家怎么模拟文本检索还想让大家学到什么呢————用类型和迭代器把思考过程写出来，而不是塞进注释里，当然在这个阶段我们鼓励大家多写注释！多写注释！多写注释！
//这个也是Rust在被设计时希望大家学到的。Rust的类型系统和迭代器可以把思考过程写出来，代码本身就能说明问题，而不是靠注释。

//看对比：
//（这里注释看不清的话，可以Ctrl+Shift去掉注释再看）
//版本 A：代码结构非扁平，逻辑塞进注释里
        // fn search(query: &str, documents: &[Document]) -> Vec<Document> {
        //     // 第一步：把查询拆成词，去掉短词，转小写，去重
        //     let mut keywords = Vec::new();
        //     for word in query.split(|c: char| c.is_whitespace() || c.is_ascii_punctuation()) {
        //         if word.chars().count() >= 2 {
        //             // 检查是否已经存在，实现去重
        //             let lower = word.to_lowercase();
        //             if !keywords.contains(&lower) {
        //                 keywords.push(lower);
        //             }
        //         }
        //     }

        //     // 第二步：遍历文档，计算每篇的匹配分数
        //     let mut matches = Vec::new();
        //     for document in documents.iter() {
        //         let searchable = format!("{} {}", document.title, document.content).to_lowercase();

        //         // 统计命中的关键词数量
        //         let mut score = 0;
        //         for keyword in &keywords {
        //             if searchable.contains(keyword.as_str()) {
        //                 score += 1;
        //             }
        //         }

        //         // 分数大于 0 才加入结果
        //         if score > 0 {
        //             let snippet = make_snippet(&document.content, /* ... */);
        //             matches.push((score, document.title.clone(), snippet));
        //         }
        //     }

        //     // 第三步：按分数降序排列
        //     matches.sort_by(|a, b| b.0.cmp(&a.0));

        //     // 第四步：丢掉分数，只返回文档
        //     let mut results = Vec::new();
        //     for (_, title, snippet) in matches {
        //         results.push(Document { title, content: snippet });
        //     }
        //     results
        // }

//版本B：代码结构扁平，逻辑清晰，不用写太多注释也能清楚函数功能
        // fn search(query: &str, documents: &[Document]) -> Vec<Document> {
        //     let keywords: HashSet<String> = query
        //         .split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
        //         .filter(|word| word.chars().count() >= 2)
        //         .map(str::to_lowercase)
        //         .collect();

        //     let mut matches: Vec<(usize, Document)> = documents
        //         .iter()
        //         .filter_map(|document| {
        //             let searchable = format!("{} {}", document.title, document.content).to_lowercase();
        //             let score = keywords
        //                 .iter()
        //                 .filter(|keyword| searchable.contains(keyword.as_str()))
        //                 .count();
        //             (score > 0).then(|| (score, document.clone()))
        //         })
        //         .collect();

        //     matches.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
        //     matches.into_iter().map(|(_, document)| document).collect()
        // }

//版本 A 的逻辑藏在控制流里——if、for、while。你要理解这段代码，必须跟踪执行路径。
//版本 B 的逻辑写在数据流里——数据从 split 流到 filter 流到 map 流到 collect。你要理解这段代码，只需从左往右读。
//不是"不要写注释"，而是"让代码本身成为注释"