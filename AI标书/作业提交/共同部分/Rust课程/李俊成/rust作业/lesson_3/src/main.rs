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