use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::fs;

// ================================================================
// Part 1 — 配置加载器
// ================================================================

/// 应用配置
#[derive(Debug)]
struct Config {
    api_key: String,
    model: String,
    max_tokens: u32,
}

impl Config {
    /// 从 key=value 格式的配置文件加载
    ///
    /// 错误处理三步曲：
    /// - `?`       → 传播错误（把错误沿调用栈向上扔）
    /// - `.context()` → 给错误"贴标签"，说明当时在做什么
    /// - `bail!()`   → 主动报告验证失败
    fn from_file(path: &str) -> Result<Self> {
        // ------------- ① 读文件 + 传播错误 + 附加上下文 -------------
        let content = fs::read_to_string(path)
            .with_context(|| format!("无法读取配置文件 \"{}\"", path))?;

        let mut api_key: Option<String> = None;
        let mut model: Option<String> = None;
        let mut max_tokens: Option<u32> = None;

        // ------------- ② 逐行解析 -------------
        for (lineno, line) in content.lines().enumerate() {
            let line = line.trim();

            // 跳过空行和注释
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // 按 '=' 切分，格式不对 → bail! 立即报错
            let (key, value) = line
                .split_once('=')
                .with_context(|| format!("第 {} 行格式错误（应为 key=value）: \"{}\"", lineno + 1, line))?;

            let key = key.trim();
            let value = value.trim();

            match key {
                "api_key" => api_key = Some(value.to_string()),
                "model" => model = Some(value.to_string()),
                "max_tokens" => {
                    // parse() 可能失败 → ? 传播 + context 附加
                    max_tokens = Some(
                        value
                            .parse::<u32>()
                            .with_context(|| format!("第 {} 行 max_tokens 不是有效数字: \"{}\"", lineno + 1, value))?,
                    );
                }
                unknown => {
                    // 未知 key → 温和提示（不中断）
                    eprintln!("⚠  第 {} 行: 未知配置项 \"{}\"，已忽略", lineno + 1, unknown);
                }
            }
        }

        // ------------- ③ 验证必填字段 -------------
        let api_key = api_key.context("缺少必填配置项: api_key")?;
        let model = model.context("缺少必填配置项: model")?;
        let max_tokens = max_tokens.context("缺少必填配置项: max_tokens")?;

        // ------------- ④ 验证字段约束 -------------
        if api_key.is_empty() {
            bail!("api_key 不能为空字符串");
        }
        if max_tokens == 0 {
            bail!("max_tokens 必须大于 0，当前值为 0");
        }

        Ok(Config {
            api_key,
            model,
            max_tokens,
        })
    }
}

// ================================================================
// Part 2 — 可扩展的命令系统
// ================================================================

/// 命令 trait — 所有命令必须遵守的契约
trait Command {
    /// 命令名称（用作查找 key）
    fn name(&self) -> &str;
    /// 执行命令，返回结果字符串
    fn run(&self, args: &[String]) -> String;
}

// ---- 命令 1: Echo ----

struct EchoCommand;

impl Command for EchoCommand {
    fn name(&self) -> &str {
        "echo"
    }

    fn run(&self, args: &[String]) -> String {
        if args.is_empty() {
            "(echo: 无参数)".to_string()
        } else {
            args.join(" ")
        }
    }
}

// ---- 命令 2: Uppercase ----

struct UppercaseCommand;

impl Command for UppercaseCommand {
    fn name(&self) -> &str {
        "uppercase"
    }

    fn run(&self, args: &[String]) -> String {
        if args.is_empty() {
            "(uppercase: 无参数)".to_string()
        } else {
            args.join(" ").to_uppercase()
        }
    }
}

// ---- 命令注册表 ----

struct CommandRegistry {
    /// key=命令名, value=命令对象（trait object，堆上分配）
    commands: HashMap<String, Box<dyn Command>>,
}

impl CommandRegistry {
    fn new() -> Self {
        CommandRegistry {
            commands: HashMap::new(),
        }
    }

    /// 注册一个命令（同名的后注册会覆盖先注册）
    fn register(&mut self, cmd: Box<dyn Command>) {
        let name = cmd.name().to_string();
        self.commands.insert(name, cmd);
    }

    /// 按名称执行命令，找不到则返回错误
    fn execute(&self, name: &str, args: &[String]) -> Result<String> {
        let cmd = self
            .commands
            .get(name)
            .with_context(|| format!("未知命令 \"{}\"，可用命令: {}", name, self.list_names()))?;
        Ok(cmd.run(args))
    }

    /// 列出所有已注册的命令名
    fn list_names(&self) -> String {
        let mut names: Vec<&str> = self.commands.keys().map(|s| s.as_str()).collect();
        names.sort();
        names.join(", ")
    }
}

// ================================================================
// main — 测试驱动
// ================================================================
fn main() {
    println!("╔══════════════════════════════════════════╗");
    println!("║   配置加载器 + 命令执行器 测试           ║");
    println!("╚══════════════════════════════════════════╝\n");

    // =================== 测试配置加载 ===================
    println!("━━━ Part 1: 配置加载 ━━━\n");

    // 成功场景
    match Config::from_file("config.txt") {
        Ok(cfg) => println!("✅ 配置加载成功:\n   {:#?}\n", cfg),
        Err(e) => println!("❌ 配置加载失败:\n   {:?}\n", e),
    }

    // 失败场景演示
    println!("--- 失败场景演示 ---\n");

    let test_cases = [
        ("不存在的文件", "nonexistent.txt"),
        ("格式错误", "config_bad_format.txt"),
        ("空 api_key", "config_empty_key.txt"),
        ("max_tokens=0", "config_zero_tokens.txt"),
    ];

    for (desc, path) in test_cases {
        match Config::from_file(path) {
            Ok(_) => println!("✅ {}: 意外成功", desc),
            Err(e) => {
                // anyhow 的错误链用 {:#} 打印，会逐层展开
                println!("❌ {}:", desc);
                println!("   最底层原因: {}", e.root_cause());
                // 打印调用链（context 逐层包裹的信息）
                for (_i, ctx) in e.chain().skip(1).enumerate() {
                    println!("      └─ {}", ctx);
                }
                println!();
            }
        }
    }

    // =================== 测试命令系统 ===================
    println!("━━━ Part 2: 命令系统 ━━━\n");

    let mut registry = CommandRegistry::new();
    registry.register(Box::new(EchoCommand));
    registry.register(Box::new(UppercaseCommand));

    println!("已注册命令: {}\n", registry.list_names());

    let test_cmds: &[(&str, &[&str])] = &[
        ("echo", &["你好", "世界"]),
        ("echo", &[]),
        ("uppercase", &["hello", "rust"]),
        ("uppercase", &["错误处理", "真有用"]),
        ("nonexistent", &[]), // 测试未知命令
    ];

    for (name, args) in test_cmds {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        match registry.execute(name, &args) {
            Ok(result) => println!("✅ {} {:?} → \"{}\"", name, args, result),
            Err(e) => println!("❌ {}: {:#}", name, e),
        }
    }
}
