use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::fs;

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

        let mut api_key = None;
        let mut model = None;
        let mut max_tokens = None;

        for (line_num, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.splitn(2, '=').collect();
            if parts.len() != 2 {
                bail!("第 {} 行格式错误，应为 key=value", line_num + 1);
            }

            let key = parts[0].trim();
            let value = parts[1].trim();

            match key {
                "api_key" => api_key = Some(value.to_string()),
                "model" => model = Some(value.to_string()),
                "max_tokens" => {
                    max_tokens = Some(value.parse().with_context(|| {
                        format!("第 {} 行 max_tokens 不是有效数字", line_num + 1)
                    })?)
                }
                _ => bail!("第 {} 行未知配置项: {}", line_num + 1, key),
            }
        }

        let api_key = api_key.context("缺少 api_key 配置")?;
        let model = model.context("缺少 model 配置")?;
        let max_tokens = max_tokens.context("缺少 max_tokens 配置")?;

        if api_key.is_empty() {
            bail!("api_key 不能为空");
        }
        if max_tokens == 0 {
            bail!("max_tokens 必须大于 0");
        }

        Ok(Self {
            api_key,
            model,
            max_tokens,
        })
    }
}

trait Command {
    fn name(&self) -> &str;
    fn run(&self, args: &[String]) -> String;
}

struct EchoCommand;

impl Command for EchoCommand {
    fn name(&self) -> &str {
        "echo"
    }

    fn run(&self, args: &[String]) -> String {
        args.join(" ")
    }
}

struct UppercaseCommand;

impl Command for UppercaseCommand {
    fn name(&self) -> &str {
        "uppercase"
    }

    fn run(&self, args: &[String]) -> String {
        args.join(" ").to_uppercase()
    }
}

struct CommandRegistry {
    commands: HashMap<String, Box<dyn Command>>,
}

impl CommandRegistry {
    fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    fn register(&mut self, command: Box<dyn Command>) {
        self.commands.insert(command.name().to_string(), command);
    }

    fn execute(&self, name: &str, args: &[String]) -> Option<String> {
        self.commands.get(name).map(|cmd| cmd.run(args))
    }
}

fn main() -> Result<()> {
    let config = match Config::from_file("config.txt") {
        Ok(c) => c,
        Err(e) => {
            eprintln!("配置加载失败: {}", e);
            return Ok(());
        }
    };
    println!("配置加载成功: {:?}", config);

    let mut registry = CommandRegistry::new();
    registry.register(Box::new(EchoCommand));
    registry.register(Box::new(UppercaseCommand));

    let tests = vec![
        ("echo", vec!["你好".to_string(), "世界".to_string()]),
        ("uppercase", vec!["hello".to_string(), "rust".to_string()]),
        ("echo", vec!["测试".to_string()]),
        ("nonexistent", vec!["args".to_string()]),
    ];

    for (cmd_name, args) in tests {
        let result = registry.execute(cmd_name, &args);
        match result {
            Some(output) => println!("{} {} → {}", cmd_name, args.join(" "), output),
            None => println!("{} → 未知命令", cmd_name),
        }
    }

    Ok(())
}
