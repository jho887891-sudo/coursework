use std::collections::HashMap;
use std::fs;
use anyhow::{Context, bail, Result};

// ============ Part 1: 配置加载 ============

#[derive(Debug)]
struct Config {
    api_key: String,
    model: String,
    max_tokens: u32,
}

impl Config {
    fn from_file(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("无法读取配置文件: {}", path))?;

        let mut api_key = String::new();
        let mut model = String::new();
        let mut max_tokens = 0u32;

        for (lineno, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line.split_once('=')
                .with_context(|| format!("第 {} 行格式错误（应为 key=value）: {}", lineno + 1, line))?;
            let key = key.trim();
            let value = value.trim();
            match key {
                "api_key" => api_key = value.to_string(),
                "model" => model = value.to_string(),
                "max_tokens" => {
                    max_tokens = value.parse()
                        .with_context(|| format!("max_tokens 不是有效数字: {}", value))?;
                }
                _ => bail!("未知配置项 '{}' 在第 {} 行", key, lineno + 1),
            }
        }

        anyhow::ensure!(!api_key.is_empty(), "api_key 不能为空");
        anyhow::ensure!(max_tokens > 0, "max_tokens 必须大于 0");

        Ok(Config { api_key, model, max_tokens })
    }
}

// ============ Part 2: 命令系统 ============

trait Command {
    fn name(&self) -> &str;
    fn run(&self, args: &[String]) -> String;
}

struct EchoCommand;
impl Command for EchoCommand {
    fn name(&self) -> &str { "echo" }
    fn run(&self, args: &[String]) -> String {
        args.join(" ")
    }
}

struct UppercaseCommand;
impl Command for UppercaseCommand {
    fn name(&self) -> &str { "uppercase" }
    fn run(&self, args: &[String]) -> String {
        args.join(" ").to_uppercase()
    }
}

struct CommandRegistry {
    commands: HashMap<String, Box<dyn Command>>,
}

impl CommandRegistry {
    fn new() -> Self {
        CommandRegistry { commands: HashMap::new() }
    }

    fn register(&mut self, cmd: Box<dyn Command>) {
        let name = cmd.name().to_string();
        self.commands.insert(name, cmd);
    }

    fn execute(&self, name: &str, args: &[String]) -> Result<String> {
        self.commands.get(name)
            .map(|cmd| cmd.run(args))
            .context(format!("未知命令: '{}'", name))
    }
}

fn main() -> Result<()> {
    // 先测试配置加载
    let cfg = Config::from_file("config.txt").context("配置加载失败")?;
    println!("配置加载成功: {:?}\n", cfg);

    // 测试命令系统
    let mut registry = CommandRegistry::new();
    registry.register(Box::new(EchoCommand));
    registry.register(Box::new(UppercaseCommand));

    let tests = vec![
        ("echo", vec!["Hello".to_string(), "World".to_string()]),
        ("uppercase", vec!["hello".to_string(), "world".to_string()]),
    ];

    for (name, args) in &tests {
        let result = registry.execute(name, args)?;
        println!("{} => {}", name, result);
    }

    // 测试未知命令
    match registry.execute("unknown", &[]) {
        Err(e) => println!("\n预期错误: {}", e),
        Ok(_) => println!("\n错误：应该返回错误但成功了"),
    }

    Ok(())
}
