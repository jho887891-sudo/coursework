use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::fs;

#[derive(Debug)]
struct Config {
    api_key: String,
    model: String,
    max_tokens: u32,
}
//1.加载配置
impl Config {
    fn from_file(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path).with_context(|| format!("无法读取配置文件: {}", path))?;

        let mut map: HashMap<String, String> = HashMap::new();
        
        for line in content.lines() {
            if line.is_empty() || line.starts_with('#') { continue; }

            if let Some((key, value)) = line.split_once('=') {
                map.insert(key.trim().to_string(), value.trim().to_string());
            }
        }

        let api_key = map.get("api_key").context("配置中缺少 'api_key' 字段")?.clone();
            
        if api_key.is_empty() {
            bail!("'api_key' 不能为空");
        }

        let model = map.get("model").context("配置中缺少 'model' 字段")?.clone();

        let max_tokens_str = map.get("max_tokens").context("配置中缺少 'max_tokens' 字段")?;
            
        let max_tokens: u32 = max_tokens_str.parse().with_context(|| format!("'max_tokens' 必须是数字，当前值为: {}", max_tokens_str))?;
            
        if max_tokens == 0 {
            bail!("'max_tokens' 必须大于 0");
        }

        Ok(Config {
            api_key,
            model,
            max_tokens,
        })
    }
}

//2.命令系统
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
    fn name(&self) -> &str { "upper" }
    fn run(&self, args: &[String]) -> String {
        args.join(" ").to_uppercase()
    }
}

struct CommandRegistry {
    map: HashMap<String, Box<dyn Command>>,
}

impl CommandRegistry {
    fn new() -> Self {
        Self { map: HashMap::new() }
    }

    fn register(&mut self, cmd: Box<dyn Command>) {
        let name = cmd.name().to_string();
        self.map.insert(name, cmd);
    }

    fn execute(&self, name: &str, args: &[String]) {
        match self.map.get(name) {
            Some(cmd) => {
                let result = cmd.run(args);
                println!("[执行结果] {}", result);
            }
            None => println!("[错误] 未找到命令: {}", name),
        }
    }
}

fn main() -> Result<()> {
    // 1.配置加载
    let config = Config::from_file("config.txt")?;
    println!("配置内容：{:?}",config);

    // 2.命令系统
    let mut registry = CommandRegistry::new();

    // 注册命令
    registry.register(Box::new(EchoCommand));
    registry.register(Box::new(UppercaseCommand));

    // 执行命令
    registry.execute("echo", &vec!["hello".to_string(), "world".to_string()]);
    registry.execute("upper", &vec!["hello".to_string(), "rust".to_string()]);
    registry.execute("unknown", &vec![]);

    Ok(())
}