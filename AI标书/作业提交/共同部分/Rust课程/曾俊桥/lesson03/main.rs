use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Config {
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
}

impl Config {
    pub fn from_file(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("读取配置文件 {} 失败", path))?;

        let mut api_key = String::new();
        let mut model = String::new();
        let mut max_tokens: Option<u32> = None;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split('=').collect();
            if parts.len() != 2 {
                bail!("无效配置行: {}，格式需为 key=value", line);
            }
            let key = parts[0].trim();
            let value = parts[1].trim();

            match key {
                "api_key" => api_key = value.to_string(),
                "model" => model = value.to_string(),
                "max_tokens" => {
                    let num = value
                        .parse::<u32>()
                        .with_context(|| format!("max_tokens 必须是合法正整数，当前值:{}", value))?;
                    max_tokens = Some(num);
                }
                _ => eprintln!("忽略未知配置项: {}", key),
            }
        }

        // 校验字段
        if api_key.is_empty() {
            bail!("api_key 不能为空");
        }
        let max_tokens = max_tokens.ok_or_else(|| anyhow!("缺少 max_tokens 配置项"))?;
        if max_tokens == 0 {
            bail!("max_tokens 必须大于 0");
        }
        if model.is_empty() {
            bail!("model 不能为空");
        }

        Ok(Self {
            api_key,
            model,
            max_tokens,
        })
    }
}

// Command 特征
trait Command {
    fn name(&self) -> &str;
    fn run(&self, args: &[String]) -> String;
}

// EchoCommand：拼接参数
struct EchoCommand;
impl Command for EchoCommand {
    fn name(&self) -> &str {
        "echo"
    }
    fn run(&self, args: &[String]) -> String {
        args.join(" ")
    }
}

// UppercaseCommand：转大写
struct UppercaseCommand;
impl Command for UppercaseCommand {
    fn name(&self) -> &str {
        "uppercase"
    }
    fn run(&self, args: &[String]) -> String {
        args.join(" ").to_uppercase()
    }
}

// WordCountCommand：统计单词总数
struct WordCountCommand;
impl Command for WordCountCommand {
    fn name(&self) -> &str {
        "wordcount"
    }
    fn run(&self, args: &[String]) -> String {
        let total: usize = args.iter().map(|s| s.split_whitespace().count()).sum();
        format!("单词总数: {}", total)
    }
}

// 命令注册器
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

    fn execute(&self, cmd_name: &str, args: &[String]) -> Result<String> {
        self.commands
            .get(cmd_name)
            .map(|cmd| cmd.run(args))
            .ok_or_else(|| anyhow!("命令 {} 不存在", cmd_name))
    }
}

///main函数：用于测试命令注册和执行功能
fn main() -> Result<()> {
    // 1. 加载配置
    match Config::from_file("config.txt") {
        Ok(config) => println!("加载配置成功:\n{:#?}", config),
        Err(e) => eprintln!("配置加载失败: {:#}", e),
    }
    println!("---");

    // 2. 注册命令
    let mut registry = CommandRegistry::new();
    registry.register(Box::new(EchoCommand));
    registry.register(Box::new(UppercaseCommand));
    registry.register(Box::new(WordCountCommand));

    // 3. 测试命令
    let echo_args = vec!["hello".into(), "rust".into()];
    println!("echo 结果: {}", registry.execute("echo", &echo_args)?);

    let upper_args = vec!["hello".into(), "world".into()];
    println!("uppercase 结果: {}", registry.execute("uppercase", &upper_args)?);

    let wc_args = vec!["I am learning rust".into()];
    println!("wordcount 结果: {}", registry.execute("wordcount", &wc_args)?);

    // 测试不存在命令
    match registry.execute("unknown", &[]) {
        Ok(res) => println!("{}", res),
        Err(e) => println!("错误: {:#}", e),
    }

    Ok(())
}