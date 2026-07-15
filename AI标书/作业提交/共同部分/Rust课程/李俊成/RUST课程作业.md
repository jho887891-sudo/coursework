# RUST课程作业

## 课程1

代码

```rust
#[derive(Debug)]
enum Grade {
    Excellent,
    Good,
    Pass,
    Fail,
}

#[derive(Debug)]
struct Student {
    name: String,
    scores: Vec<u32>,
}

impl Student {
    // 计算平均分。
    fn average(&self) -> f64 {
        if self.scores.is_empty() {
            return 0.0;
        }
        let mut sum = 0;
        for score in &self.scores {
            sum += score;
        }
        //做类型强转
        sum as f64 / self.scores.len() as f64
    }

    // 根据平均分算出等级
    fn grade(&self) -> Grade {
        let avg = self.average();
        if avg >= 90.0 {
            Grade::Excellent
        } else if avg >= 75.0 {
            Grade::Good
        } else if avg >= 60.0 {
            Grade::Pass
        } else {
            Grade::Fail
        }
    }
}

fn main() {
    // 用 .into() 把 &str 变成 String
    let s1 = Student {
        name: "小明".into(),
        scores: vec![90, 95, 88],
    };
    let s2 = Student {
        name: "小红".into(),
        scores: vec![78, 82, 75],
    };
    let s3 = Student {
        name: "小刚".into(),
        scores: vec![50, 60, 55],
    };

    let students = vec![s1, s2, s3];

    for s in &students {
        println!("姓名: {}, 平均分: {:.1}, 等级: {:?}", s.name, s.average(), s.grade());
    }
}

```



### 运行截图

![image-20260714211929072](./lesson_1/lesson1_运行结果.png)

















## 课程2

代码

```rust

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
```





![image-20260714211825726](./lesson_2/lesson2.png)



## 课程3

代码

```rust
use std::collections::HashMap;
use std::fs;
use anyhow::{bail, Context, Result};

// 配置加载

//定义 Config 结构体
#[derive(Debug)]
struct Config {
    api_key: String,
    model: String,
    max_tokens: u32,
}

impl Config {
    // 实现从文件加载配置的方法
    fn from_file(path: &str) -> anyhow::Result<Self> {
        // 读文件，用 ? 传播错误，用 .context() 附加错误上下文
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read file at: {}", path))?;

        let mut api_key = String::new();
        let mut model = String::new();
        let mut max_tokens = 0;

        // 按行解析
        for (line_num, line) in content.lines().enumerate() {
            let line = line.trim();
            // 跳过空行和注释
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // 按等号分割格式为 key=value 的行
            let parts: Vec<&str> = line.splitn(2, '=').collect();
            if parts.len() != 2 {
                bail!("Invalid format at line {}: missing '='", line_num + 1);
            }

            let key = parts[0].trim();
            let value = parts[1].trim();

            match key {
                "api_key" => api_key = value.to_string(),
                "model" => model = value.to_string(),
                "max_tokens" => {
                    // 解析数字，失败时通过 ? 传播
                    max_tokens = value.parse::<u32>()
                        .with_context(|| format!("Invalid max_tokens value '{}' at line {}", value, line_num + 1))?;
                }
                _ => {} // 忽略其他未知配置
            }
        }

        // 验证字段，验证失败用 bail! 报告
        if api_key.is_empty() {
            bail!("Configuration error: api_key cannot be empty");
        }
        if max_tokens == 0 {
            bail!("Configuration error: max_tokens must be greater than 0");
        }

        Ok(Config {
            api_key,
            model,
            max_tokens,
        })
    }
}

// 定义 Command trait
trait Command {
    fn name(&self) -> &str;
    fn run(&self, args: &[String]) -> String;
}

// 实现 EchoCommand（拼接参数）
struct EchoCommand;
impl Command for EchoCommand {
    fn name(&self) -> &str {
        "echo"
    }
    fn run(&self, args: &[String]) -> String {
        args.join(" ")
    }
}

// 实现 UppercaseCommand（转大写）
struct UppercaseCommand;
impl Command for UppercaseCommand {
    fn name(&self) -> &str {
        "uppercase"
    }
    fn run(&self, args: &[String]) -> String {
        args.join(" ").to_uppercase()
    }
}

// 实现 CommandRegistry
struct CommandRegistry {
    commands: HashMap<String, Box<dyn Command>>,
}

impl CommandRegistry {
    fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    fn register(&mut self, cmd: Box<dyn Command>) {
        self.commands.insert(cmd.name().to_string(), cmd);
    }

    fn execute(&self, name: &str, args: &[String]) -> Option<String> {
        self.commands.get(name).map(|cmd| cmd.run(args))
    }
}


fn main() -> anyhow::Result<()> {
    //  config.txt 的文件
    // 文件内容格式为：
    // api_key= 
    // model=  
    // max_tokens=  
    
    println!("Loading config.txt...");
    let config = Config::from_file("src/config.txt")?;
    println!("Loaded Config: {:?}", config);
    println!("-----------------------------------");

    // 初始化命令注册表并注册基本要求的两个命令
    let mut registry = CommandRegistry::new();
    registry.register(Box::new(EchoCommand));
    registry.register(Box::new(UppercaseCommand));

    let test_args = vec!["hello".to_string(), "rust".to_string()];

    // 测试命令执行
    if let Some(res) = registry.execute("echo", &test_args) {
        println!("echo result: {}", res);
    }

    if let Some(res) = registry.execute("uppercase", &test_args) {
        println!("uppercase result: {}", res);
    }

    Ok(())
}
```







### 运行截图

![image-20260714213700880](./lesson_3/lesson3.png)





