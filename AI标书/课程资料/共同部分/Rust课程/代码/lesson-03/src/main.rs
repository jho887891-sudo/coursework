// 课程问题：
// 程序启动时要读配置文件，用户可能写错（路径不存在、格式不对、数字写成 "many"）。
// 同时，程序要支持多种命令（echo、uppercase、wc...），注册中心如何统一调用完全不同的命令？
//
// 这个课程的设计是让大家能回答两个核心问题：
//   1. 出错了怎么办？  → Result + ? + .context()
//   2. 不同东西怎么统一调用？ → Trait + Box<dyn Trait>

use anyhow::{Context, Result, bail};
use std::{collections::HashMap, fs};

// ── 配置文件的结构 ──
#[derive(Debug)]
struct Config {
    api_key: String,
    model: String,
    max_tokens: u32,
}

impl Config {
    // ── 第一层：读文件 ──
    // fs::read_to_string(path) 返回 Result<String, io::Error>
    //   - 文件存在且可读 → Ok(文件内容)
    //   - 文件不存在 / 权限不够 / 不是 UTF-8 → Err(...)
    fn from_file(path: &str) -> Result<Self> {
        let content =
            fs::read_to_string(path).with_context(|| format!("无法读取配置文件：{path}"))?;
        //                                     ↑
        //           with_context 给底层 IO 错误加一层人类可读的解释：
        //           "无法读取配置文件：./config.example"
        //           而不是只扔一个冷冰冰的 "No such file or directory"
        Self::parse(&content)
    }

    // ── 第二层：解析内容 ──
    // 文件内容是一个字符串，每行 "key=value"，需要变成 HashMap。
    // 但每一行都可能格式错误（缺少 '='）、每一值都可能类型错误（max_tokens 不是数字）。
    // 每一步都有可能出错——用 ? 逐层传递，用 context 逐层加解释。
    fn parse(content: &str) -> Result<Self> {
        // ── 子流水线：文本行 → HashMap<&str, &str> ──
        //
        //   content.lines()                         按行切分
        //   │ filter(!trim.is_empty)                 跳过空行
        //   │ map(split_once('='))                   每行按 '=' 拆成 (key, value)
        //   │ collect::<Result<HashMap<_, _>>>()     全成功 → HashMap，有一个失败 → 直接返回 Err
        //   ▼
        //   {"api_key": "demo-key", "model": "qwen-plus", "max_tokens": "many"}
        //
        // collect::<Result<_>>() 的特殊行为：
        //   碰到 Ok 就攒着，碰到第一个 Err 就立刻返回那个 Err。
        //   不需要写 for 循环去手动检查。类型替你做了决定。
        let values: HashMap<&str, &str> = content
            .lines()
            .filter(|line| !line.trim().is_empty())    // 空行不解析
            .map(|line| {
                line.split_once('=')
                    .with_context(|| format!("配置行缺少 '='：{line}"))
                //  ↑ 如果某行没有 '='，错误信息告诉你是"哪一行"出了问题
            })
            .collect::<Result<_>>()?;  // ← 注意这个 ?，把第一个解析错误向上抛

        // ── 逐字段校验 ──
        // 每个字段都可能缺、可能格式不对。三层防御：
        //   1. required()：字段在不在 → "缺少配置项：xxx"
        //   2. .parse() / .is_empty()：值对不对 → "max_tokens 必须是正整数"
        //   3. 业务校验：=0 也不行 → "max_tokens 必须大于 0"
        let api_key = required(&values, "api_key")?;
        if api_key.is_empty() {
            bail!("api_key 不能为空");
            // bail! 是 return Err(anyhow!(...)) 的语法糖
            // 立即退出函数，把错误交还给调用者
        }

        let max_tokens = required(&values, "max_tokens")?
            .parse::<u32>()
            .context("max_tokens 必须是正整数")?;
            //          ↑
            //  底层错误：invalid digit found in string （技术事实）
            //  context：max_tokens 必须是正整数         （怎么修）
            //  两层各服务不同读者：运维看外层修配置，开发看内层定位原因
        if max_tokens == 0 {
            bail!("max_tokens 必须大于 0");
        }

        Ok(Self {
            api_key: api_key.into(),    // &str → String
            model: required(&values, "model")?.into(),
            max_tokens,
        })
    }
}

// ── 辅助函数：从 HashMap 里取必填字段 ──
// HashMap::get() 返回 Option<&V>——可能没有，但不一定是"错误"。
// 在这里我们把"没有"翻译成"错误"，加上 key 的名字：
//   "缺少配置项：api_key"
fn required<'a>(values: &HashMap<&str, &'a str>, key: &str) -> Result<&'a str> {
    values
        .get(key)
        .copied()      // Option<&&str> → Option<&str>
        .with_context(|| format!("缺少配置项：{key}"))
        //  ↑ 把 Option 的 None 变成 Result 的 Err，附上明确的上下文
}

// ════════════════════════════════════════════════════════════════
// 第二部分：Trait —— 你不知道我是谁，但你知道我能做什么（类似于Java的接口）
// ════════════════════════════════════════════════════════════════

// Command trait 是一份行为合约：
//   实现了 Command 的类型，必须提供 name() 和 run() 两个方法。
//
// 注册中心不关心你是什么结构体、里面有什么字段——
// 它只关心：名字和返回结果。
trait Command {
    fn name(&self) -> &str;
    fn run(&self, args: &[String]) -> String;
}

// 三个命令，三种完全不同的类型，同一个 trait。
// 结构体是空的——它们不需要字段，实现本身就够了。
struct EchoCommand;
struct UppercaseCommand;
struct WordCountCommand;

impl Command for EchoCommand {
    fn name(&self) -> &str {
        "echo"
    }
    fn run(&self, args: &[String]) -> String {
        args.join(" ")
    }
}

impl Command for UppercaseCommand {
    fn name(&self) -> &str {
        "uppercase"
    }
    fn run(&self, args: &[String]) -> String {
        args.join(" ").to_uppercase()
    }
}

impl Command for WordCountCommand {
    fn name(&self) -> &str {
        "wc"
    }
    fn run(&self, args: &[String]) -> String {
        let count = args.join(" ").split_whitespace().count();
        format!("{} 个词", count)
    }
}

// ── 注册中心：只认合约，不认具体类型 ──
//
//   HashMap<String, Box<dyn Command>>
//
//   拆开看：
//     String         → 命令名 "echo" / "uppercase" / "wc"
//     Box<...>       → 堆上分配，所有命令指针大小相同
//     dyn Command    → 擦除具体类型，只保留 trait 定义的行为
//
//   内存布局：
//
//     HashMap                    堆
//     ┌──────────────────┐      ┌─────────────────┐
//     │ "echo" ──→ Box ──→│────→│ EchoCommand     │ (零大小，但 Box 有虚表指针)
//     │ "wc"   ──→ Box ──→│────→│ WordCountCommand│
//     └──────────────────┘      └─────────────────┘
//
//   Box<dyn Command> 抹掉了具体类型，只保留了"它一定能执行命令"这个能力。
//   加新命令不需要改注册中心——这就是扩展性。
struct CommandRegistry {
    commands: HashMap<String, Box<dyn Command>>,
}

impl CommandRegistry {
    fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    // register 接受 impl Command + 'static：
    //   impl Command  → 任何实现了 Command 的类型（编译时为每种类型生成一份代码）
    //   + 'static     → 命令不能包含借用引用——放进 HashMap 的东西必须自己拥有数据
    //
    // 进了 Box::new(command) 之后，具体类型被擦除，变成 Box<dyn Command>。
    // register 走静态分发（编译时单态化），execute 走动态分发（运行时查虚表）。
    fn register(&mut self, command: impl Command + 'static) {
        self.commands
            .insert(command.name().into(), Box::new(command));
    }

    fn execute(&self, name: &str, args: &[String]) -> Result<String> {
        // HashMap::get 返回 Option —— 命令可能不存在，正常情况，不是"错误"
        // with_context 把 None 翻译成 Result::Err，写明是哪个命令不存在
        let command = self
            .commands
            .get(name)
            .with_context(|| format!("未知命令：{name}"))?;
        Ok(command.run(args))
        //      ↑ 动态分发：运行时查虚表，找到真正的 run() 实现
    }
}

// ════════════════════════════════════════════════════════════════
// main() → Result<()> 意味着 main 也能用 ? 优雅处理错误
// ════════════════════════════════════════════════════════════════
fn main() -> Result<()> {
    // concat! + env! 在编译时拼接项目根目录 + 文件名，避免硬编码路径
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/config.example");
    let config = Config::from_file(path)?;  // ← 配置解析失败，main 直接退出并打印错误

    println!(
        "配置：model={}, max_tokens={}, api_key 已加载={}",
        config.model,
        config.max_tokens,
        !config.api_key.is_empty()   // 真实项目中不要打印密钥本身
    );

    let mut registry = CommandRegistry::new();
    registry.register(EchoCommand);
    registry.register(UppercaseCommand);
    registry.register(WordCountCommand);   // ← 新增命令，注册中心一行不改

    let args = vec!["Hello,".into(), "Rust!".into()];
    println!("echo:      {}", registry.execute("echo", &args)?);
    println!("uppercase: {}", registry.execute("uppercase", &args)?);
    println!("wc:        {}", registry.execute("wc", &args)?);
    Ok(())
}

// ════════════════════════════════════════════════════════════════
// 三类情况，三种工具
// ════════════════════════════════════════════════════════════════
//
// Option<T>     "可能没有"      例：HashMap::get 查不到 → None
//                               处理：.ok_or() / .with_context(|| ...)? 转成 Result
//
// Result<T, E>  "可能失败"      例：读文件、解析数字、找配置行
//                               处理：? 向上传递 + .context() 加解释
//
// panic!        "无法继续"      例：数组越界（是 bug）、除以零（数学无定义）
//                               不是用来处理"用户写错了配置"的！
//
// 判断原则：
//   调用者能合理处理的 → Result
//   说明代码有 bug 的 → panic!
//   作业里的普通输入错误 → 不应该 panic，应该优雅报错

#[cfg(test)]
mod tests {
    use super::*;

    // 测试验证的是行为而不是实现：
    // 不关心内部用了什么类型、怎么传的——只问：
    // "用户把 max_tokens 写成 'many' 时，错误信息里有没有告诉人正确的格式？"
    #[test]
    fn invalid_number_keeps_its_context() {
        let error = Config::parse("api_key=x\nmodel=qwen\nmax_tokens=many")
            .unwrap_err()
            .to_string();
        assert!(error.contains("max_tokens 必须是正整数"));
    }
}

// ════════════════════════════════════════════════════════════════
// 本课总结
// ════════════════════════════════════════════════════════════════
//
// 错误处理的三板斧：
//   ?           成功就拿值继续，失败就立刻退出把错误交给调用者
//   .context()  给底层错误加一层人类可读的解释（哪个文件？哪个字段？正确格式是什么？）
//   bail!()     立即返回一个带描述的 Err
//
// 用 trait 做扩展：
//   trait 是一份行为合约——只关心能做什么，不关心是什么类型
//   Box<dyn Trait> 擦除具体类型，只保留合约规定的能力
//   加新功能 → 写结构体 + 实现 trait + 注册 → 注册中心不用改
//
// 最重要的习惯：
//   错误信息写给解决问题的人看——不是"invalid digit"，而是"max_tokens 必须是正整数"
//   一个合格的程序不是不出错，而是出错了能让人快速修好
