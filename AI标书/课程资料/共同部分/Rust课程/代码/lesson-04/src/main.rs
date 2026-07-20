// 课程问题：
//   写一个命令行 AI 助手，支持问答和翻译两种模式。
//   背后调用阿里云 DashScope（通义千问）的 HTTP API。
//
// 这节课回答三个核心问题：
//   1. 为什么要把程序拆成 main + call_llm 两层？
//   2. async/await 到底是干嘛的？
//   3. 一次网络请求可能在哪三个地方出错？每种错该报什么信息？

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

// DashScope 文本生成 API 地址（通义千问 / Qwen）
const ENDPOINT: &str =
    "https://dashscope.aliyuncs.com/api/v1/services/aigc/text-generation/generation";

// ════════════════════════════════════════════════════════════════
// 第一层：命令行界面 —— 理解用户意图
// ════════════════════════════════════════════════════════════════

// clap 从结构体和 /// 注释自动生成 --help 文档。
//   #[derive(Parser)]    → 自动解析命令行参数
//   #[derive(Subcommand)] → 自动分发子命令（ask / translate）
//
// 跑一下 cargo run -- --help 就能看到完整的帮助信息，一行文档代码都没写。
#[derive(Parser)]
#[command(name = "ai-assistant", about = "一个简单的命令行 AI 助手")]
struct Cli {
    /// 模型名称（qwen-plus / qwen-turbo / qwen-max 等）
    #[arg(long, default_value = "qwen-plus", global = true)]
    model: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 回答一个问题
    Ask {
        question: String,    // 位置参数，直接跟在 ask 后面
    },
    /// 把文本翻译为目标语言
    Translate {
        text: String,        // 位置参数：要翻译的文本
        #[arg(short, long)]  // -t / --to 指定目标语言
        to: String,
    },
}

// ════════════════════════════════════════════════════════════════
// API 返回的 JSON 结构 —— 只定义我们需要的字段
// ════════════════════════════════════════════════════════════════

// serde 的 Deserialize 自动把 JSON 反序列化到这些结构体。
// JSON 里还有很多其他字段（usage、id、request_id...），
// 我们不定义它们，serde 就自动忽略——只取需要的。
//
// DashScope 返回的 JSON 结构：
//   { "output": { "choices": [{ "message": { "content": "回复文本" } }] } }
//
// 对应关系：
//   ApiResponse  →  整个 JSON
//   Output       →  .output
//   Choice       →  .choices[0]
//   Message      →  .message
//   content      →  最终需要的回复文本

#[derive(Deserialize)]
struct ApiResponse {
    output: Output,
}

#[derive(Deserialize)]
struct Output {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: String,
}

// ════════════════════════════════════════════════════════════════
// 第二层：网络通信 —— 和远程服务器对话
// ════════════════════════════════════════════════════════════════

// async fn 返回的是一个 Future——"未来会变成 Result<String> 的东西"。
// 调用者用 .await 等待它完成。
//
// async/await 要解决的根本问题是：
//   网络往返需要几百毫秒到几秒。
//   同步代码：线程卡死，什么也不做，干等。
//   异步代码：.await 把任务挂起，把线程还给 tokio 运行时，
//            运行时用这条线程去处理其他任务。
//            响应到达后，运行时唤醒这个任务，继续往下执行。
//
// 一句话：.await 表示"我需要等，但线程不必傻站着"。
//
//  注意：.await 不是自动并发。
//   单次 .await = 串行等待。
//   多个任务 + tokio::join! = 真正并发（下次课讲）。

async fn call_llm(api_key: &str, model: &str, system: &str, user: &str) -> Result<String> {
    // ── 构造请求体 ──
    // serde_json::json! 宏把 JSON 写在 Rust 代码里，编译时检查语法。
    // system prompt 设定 AI 的角色，user prompt 是用户的实际问题。
    let body = json!({
        "model": model,
        "input": { "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user }
        ]},
        "parameters": { "result_format": "message" }
    });

    // ── 三层错误防御 ──
    //
    // 一次 HTTP 请求可能在三个完全不同的环节失败。
    // 每种失败对应不同的错误信息和修复方式：
    //
    //   send().await
    //   │  失败 → "无法连接 DashScope"            ← 网络层（DNS、TLS、超时）
    //   ▼
    //   status().is_success()
    //   │  失败 → "API 返回 401：Invalid API key"  ← HTTP 层（认证、限流、服务端错误）
    //   ▼
    //   response.json::<ApiResponse>()
    //   │  失败 → "API 返回了无法解析的数据"        ← 数据格式层（JSON 结构变了）
    //   ▼
    //   choices.next()
    //   │  失败 → "API 没有返回任何回复"            ← 业务层（空结果）
    //   ▼
    //   Ok(content)
    //
    // 修 bug 的人顺着错误信息就能定位到具体环节，不需要翻源码。

    // 第一层：网络连接
    // Client::new() 创建 HTTP 客户端，.post() 指定 URL，.bearer_auth() 设 Bearer Token。
    // .json(&body) 自动序列化为 JSON 并设 Content-Type 头。
    // .send().await —— 把请求发出去，挂起等待响应。
    //   DNS 解析失败、TLS 握手失败、连接超时 → "无法连接 DashScope"
    let response = Client::new()
        .post(ENDPOINT)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .context("无法连接 DashScope")?;
        //          ↑
        //  这一层的信息告诉用户：服务器根本联系不上。
        //  不是你的 API Key 错了，不是参数格式不对——先检查网络、检查 VPN、
        //  检查 DashScope 是否宕机。

    // 第二层：HTTP 状态码
    // 连接成功，但服务器返回了非 2xx 状态码：
    //   401 → API Key 错误或过期
    //   429 → 请求太频繁，被限流
    //   500 → DashScope 内部故障
    // 错误信息同时包含状态码和响应正文——运维需要两者来定位问题。
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        //      ↑ .text() 也是 async——读取响应体可能涉及网络 I/O
        bail!("API 返回 {status}：{body}");
        //    bail! = return Err(anyhow!(...))
        //    立即退出函数，把错误交给调用者
    }

    // 第三层 + 第四层：JSON 解析 + 空结果检查
    // HTTP 200，正文拿到。但 JSON 结构是否符合预期？
    //   - API 改版 → choices 字段没了 → "API 返回了无法解析的数据"
    //   - 正常返回但 choices 是空数组 → "API 没有返回任何回复"
    let response: ApiResponse = response.json().await
        .context("API 返回了无法解析的数据")?;
        //          ↑
        //  告诉调用者：网络没问题，认证没问题，但返回的数据不符合预期结构。
        //  可能是 API 升级改了格式，可能是请求参数导致返回了其他内容。

    response
        .output
        .choices
        .into_iter()
        .next()                          // 取第一个 choice（通常只有一个）
        .map(|choice| choice.message.content)
        .context("API 没有返回任何回复")
        //          ↑
        //  系统正常，认证正常，JSON 结构正常，但没有内容。
        //  choices 是空数组——可能模型拒绝了请求（安全策略），
        //  或者参数导致模型没有生成任何文本。
}

// ════════════════════════════════════════════════════════════════
// 程序入口 —— 两层在这里汇合
// ════════════════════════════════════════════════════════════════

// #[tokio::main] 把一个普通的 async fn main 变成 tokio 运行时的入口。
// 它等价于手动创建 tokio runtime 然后 block_on(main())。
// async fn main() -> Result<()> 意味着 main 也能用 ? 和 .await。
#[tokio::main]
async fn main() -> Result<()> {
    // 加载 .env 文件。
    // CARGO_MANIFEST_DIR 在编译时就会被写入 Cargo.toml 所在目录的路径，
    // 所以不论从哪个目录运行 cargo，都能找到正确的 .env 文件。
    // .ok() 吞掉 "文件不存在" 的错误——.env 是可选的。
    dotenv::from_filename(concat!(env!("CARGO_MANIFEST_DIR"), "/.env")).ok();

    let cli = Cli::parse();    // 解析命令行参数

    // 从环境变量读取 API Key。
    // 为什么不放在配置文件里？因为密钥不应该被 git 追踪。
    // .env 在 .gitignore 里，不会提交到仓库。
    let api_key =
        std::env::var("DASHSCOPE_API_KEY").context("请在 .env 中设置 DASHSCOPE_API_KEY")?;

    // ── main 的职责：把用户意图翻译成 system prompt ──
    //
    // main 不知道 HTTP 长什么样，call_llm 不知道命令行长什么样。
    // 中间只通过两个字符串连接：system（角色设定）、user（用户输入）。
    // 将来换模型（通义千问 → Claude / GPT）→ 只改 call_llm，main 不变。
    // 将来加新命令（总结、润色…）→ 只加一个枚举变体 + match 分支，
    //                                    call_llm 不变。
    let (system, user) = match cli.command {
        Command::Ask { question } => (
            "你是一个有帮助的 AI 助手。".to_string(),
            question,
        ),
        Command::Translate { text, to } => (
            format!("你是一个翻译助手，将用户输入翻译为{to}，只输出译文。"),
            text,
        ),
    };

    let answer = call_llm(&api_key, &cli.model, &system, &user).await?;
    //                                            ↑                      ↑
    //                                     .await：等待网络返回             ?：出错就退出
    println!("{answer}");
    Ok(())
}

// ════════════════════════════════════════════════════════════════
// 本课总结
// ════════════════════════════════════════════════════════════════
//
// 两层架构：
//   main       → 理解用户命令，构造 system prompt
//   call_llm   → 构造 HTTP 请求、等待网络、解析 JSON
//   两者之间只传字符串。换模型只改 call_llm，加命令只改 main。
//
// async/await：
//   async fn   → 函数返回一个 Future，不是立即执行
//   .await     → "我需要等，但线程不必傻站着"
//   await ≠ 并发 → 单次 await 是串行，多个任务 + join 才是并发
//
// 三层错误防御（一个 HTTP 请求在三个地方可能死掉）：
//   1. 网络层     → .context("无法连接 DashScope")       DNS/TLS/超时
//   2. HTTP 层    → bail!("API 返回 {status}：{body}")   401/429/500
//   3. 数据格式层  → .context("API 返回了无法解析的数据")  JSON 不匹配
//   4. 业务层     → .context("API 没有返回任何回复")      choices 为空
//
//   每一层的信息都在回答：哪个环节出了问题？用户该怎么修？
//
// 最重要的习惯：
//   拆分函数时，不是按"代码量"分，而是按"职责"分。
//   main 不知道 HTTP 的存在，call_llm 不知道 CLI 的存在——
//   中间只传最简单的数据。这就是模块化的核心。




// ╔══════════════════════════════════════════════════════════════════════════╗
// ║  拓展：async/.await 底层全栈拆解 —— 从语法糖到 DMA 硬件                 ║
// ╚══════════════════════════════════════════════════════════════════════════╝
//
// 一句话总览：
//   async 是编译器语法糖，把函数编译成"可暂停的状态机"。
//   .await 是主动让出执行权的暂停点。
//   同步阻塞 = OS 把线程挂起，CPU 空等网络硬件。
//   异步      = 软件任务挂起，线程回收复用，网卡+DMA 独立跑传输，硬件中断唤醒任务。
//
// ════════════════════════════════════════════════════════════════════════════
// 第一层：编程语言层 —— 编译器到底做了什么
// ════════════════════════════════════════════════════════════════════════════
//
// 普通函数：调用即执行，栈是连续的，遇到阻塞系统调用直接卡住线程。
// async fn：编译器把整个函数翻译成一个状态机结构体。
//
// 三条变换规则：
//   1. 所有跨 .await 的局部变量 → 提升为结构体字段（暂停后恢复要用）
//   2. 每个 .await → 一个状态编号（暂停点）
//   3. 函数不返回实际值 → 返回一个惰性 Future（被 poll 才跑）
//
// 你写的代码：
//
//   async fn req() {
//       let resp = Client::new().post(url).send().await;   // ← .await = 暂停点
//       println!("{}", resp.status());
//   }
//
// 编译器生成的等价状态机（伪代码）：
//
//   struct ReqStateMachine {
//       state: u32,           // 0=初始, 1=等待 send 完成
//       url:   String,        // 跨 .await 的局部变量 → 变成结构体字段
//       resp:  Option<Response>,
//   }
//
//   impl Future for ReqStateMachine {
//       fn poll(&mut self, cx: &mut Context) -> Poll<()> {
//           match self.state {
//               0 => {
//                   let fut = Client::new().post(&self.url).send();
//                   match fut.poll(cx) {
//                       Poll::Pending => {           // IO 未就绪
//                           self.state = 1;           // 记住"卡在哪一步"
//                           return Poll::Pending;     // 挂起，线程还给调度器
//                       }
//                       Poll::Ready(resp) => {       // IO 已完成（运气好）
//                           self.resp = Some(resp);
//                           self.state = 1;           // 跳到下一步
//                       }
//                   }
//               }
//               1 => {
//                   println!("{}", self.resp.as_ref().unwrap().status());
//                   Poll::Ready(())                  // 状态机完成
//               }
//           }
//       }
//   }
//
// .await 的核心行为：调用 X.poll()
//   1. IO 未就绪 → 返回 Poll::Pending
//   2. 把当前任务的 Waker（唤醒句柄）注册到 epoll / io_uring
//   3. 主动让出工作线程，调度器拿线程去跑其他 async 任务
//   4. 线程不会被 OS 挂起（这是和同步阻塞的根本区别）
//
// ════════════════════════════════════════════════════════════════════════════
// 第二层：OS 内核 —— epoll / io_uring（用户态 ↔ 内核的桥梁）
// ════════════════════════════════════════════════════════════════════════════
//
// send().await 发出 HTTP POST 请求时，底层发生了什么：
//
//   1. Tokio 底层 mio 库打开非阻塞 socket fd，调用 write 发送 HTTP 报文
//   2. 网卡 DMA 把内存里的请求数据搬运到网卡硬件缓冲区（CPU 不参与拷贝）
//   3. 数据包经路由器转发几千公里到服务器
//
// ╔══════════════════════════════════════════════════════════════════╗
// ║  分岔路口：同步阻塞 vs 异步                                      ║
// ╠══════════════════════════════════════════════════════════════════╣
// ║                                                                ║
// ║  同步阻塞（BIO）：                                              ║
// ║    write/read → OS 检测 socket 无数据                           ║
// ║    → OS 把线程状态改为 TASK_UNINTERRUPTIBLE                      ║
// ║    → 线程从 CPU 运行队列移除，保存完整栈和寄存器上下文              ║
// ║    → 线程彻底休眠，直到网卡中断才被唤醒                           ║
// ║    → 1 连接 = 1 线程，栈内存大、上下文切换贵、并发上限低           ║
// ║                                                                ║
// ║  异步 + epoll（Tokio）：                                        ║
// ║    socket 设为非阻塞 → epoll_ctl 把 fd 注册进内核 epoll，绑定 Waker ║
// ║    → 任务返回 Pending，Tokio 回收线程                            ║
// ║    → 线程仍在 CPU 上活跃，只是换了个 async 任务跑                  ║
// ║    → 没有被 OS 挂起，无重型线程上下文切换                         ║
// ║    → epoll 持续监控 fd，等网卡接收响应                           ║
// ║                                                                ║
// ╚══════════════════════════════════════════════════════════════════╝
//
// ════════════════════════════════════════════════════════════════════════════
// 第三层：计网硬件 —— 网卡、DMA、硬件中断（几千公里往返的真正物理过程）
// ════════════════════════════════════════════════════════════════════════════
//
// 服务器响应报文抵达本机网卡时，完整硬件流水线：
//
//   ┌──────────┐      ┌──────────┐      ┌──────────────┐      ┌──────────┐
//   │ 网卡接收  │ ──→ │ DMA 搬运 │ ──→ │ 硬件中断 IRQ │ ──→ │ CPU 响应 │
//   │ 光/电信号  │      │ 数据 →   │      │ 通知 CPU     │      │ 进入 ISR │
//   │ → 二进制帧 │      │ 内核缓冲 │      │ "数据到了！" │      │ 标记就绪 │
//   └──────────┘      └──────────┘      └──────────────┘      └──────────┘
//
//   关键：DMA 控制器全程独立搬运数据，CPU 完全不参与数据拷贝。
//   没有 DMA 的话：CPU 必须循环读写网卡寄存器（PIO），等待期间 CPU 被占死。
//   有 DMA + 异步 IO：CPU 只发一次 IO 指令 → DMA 接管总线 → CPU 释放跑别的任务
//                     → 传输完成发中断 → 完美对应 async "等待时线程复用"。
//
// ════════════════════════════════════════════════════════════════════════════
// 第四层：从硬件中断回到用户态 —— Waker 唤醒链路
// ════════════════════════════════════════════════════════════════════════════
//
//   硬件中断触发
//   │ CPU 进入内核 ISR → 标记 socket fd 为可读 → epoll 链表记录就绪事件
//   ▼
//   Tokio Reactor 线程调用 epoll_wait → 拿到就绪 fd 列表
//   │ 根据 fd 找到之前注册的 Waker
//   ▼
//   Waker 把挂起的 async 任务推入调度器就绪队列
//   │
//   ▼
//   调度器有空闲工作线程时，取出任务
//   │ 从上次 .await 暂停的状态机恢复，继续 poll → 拿到 response
//   ▼
//   call_llm 继续执行：解析 JSON → 返回回复文本
//
//   关键对比：
//     同步：中断唤醒的是 OS 线程 → 需要完整线程上下文切换（保存/恢复寄存器+栈）
//     异步：中断唤醒的是用户态任务 → 线程一直在运行，只切换任务状态机
//           M:N 调度：十几个 OS 线程承载百万级 async 任务
//
// ════════════════════════════════════════════════════════════════════════════
// 第五层：计组视角 —— CPU、内存、总线三者关系
// ════════════════════════════════════════════════════════════════════════════
//
//                   同步阻塞 IO                   async + .await
//   ─────────────── ──────────────────────── ──────────────────────────
//   CPU 状态       线程被 OS 挂起，时间片没收   线程持续占用 CPU，切换不同任务
//   CPU 利用率     等待期间 CPU 可能空闲         等待期间 CPU 跑其他任务，打满
//   切换开销       线程级上下文切换             用户态任务状态机切换
//                 （数十~数百 CPU 周期）        （仅修改结构体字段，几乎免费）
//   内存占用       每连接 = 独立线程栈 (MB 级)   每任务 = 状态机数据 (KB 级)
//   数据搬运       可能 CPU 参与拷贝             DMA 独立完成，CPU 零参与
//   并发上限       受线程数限制（几百~几千）      受内存限制（百万级）
//
// ════════════════════════════════════════════════════════════════════════════
// 同步阻塞 vs async .await —— 一次 HTTP 请求的完整时序对比
// ════════════════════════════════════════════════════════════════════════════
//
//   同步阻塞（BIO）                         async .await（Tokio + epoll）
//   ──────────────────────                ──────────────────────────────
//   1. CPU 执行代码                         1. CPU 执行 async 任务
//   2. 调用阻塞 send()                     2. 非阻塞 socket 发送请求
//   3. 陷入内核，socket 无数据               3. epoll 注册 fd + Waker
//   4. OS 挂起线程，移出 CPU 队列 ← 卡死！    4. 任务返回 Pending，线程回收
//   5. 网卡 DMA 传输（等待…）                5. 网卡 DMA 独立传输，CPU 跑其他业务
//   6. 数据到达，网卡发硬件中断               6. 数据到达 → DMA → 硬件中断
//   7. OS 唤醒线程，放回调度队列              7. epoll 就绪 → Waker → 任务入队
//   8. CPU 分到时间片，线程恢复               8. 线程空闲时取出任务，从暂停点继续
//   9. 拿到响应                              9. 拿到响应
//
//   核心差异：
//     同步第 4 步：线程休眠，资源永久占用，并发上限 = 线程数
//     异步第 4 步：线程完全复用，无资源浪费，数万并发仅需十几个 OS 线程
//
// ════════════════════════════════════════════════════════════════════════════
// 拓展速记表
// ════════════════════════════════════════════════════════════════════════════
//
//   层次     同步阻塞做了什么               异步 async/.await 做了什么
//   ────     ──────────────────────────   ────────────────────────────────
//   语法层   函数即调用，栈连续              编译器生成状态机，Future 惰性求值
//   运行时   线程卡死在系统调用上             任务 Pending + Waker 注册 + 线程复用
//   OS 层    内核挂起线程，移出调度队列        epoll/io_uring 非阻塞监控，线程不休眠
//   硬件层   网卡 + DMA 传输，CPU 干等        网卡 + DMA 独立传输，CPU 跑其他任务
//   唤醒     中断 → 唤醒线程（重上下文切换）   中断 → Waker → 任务状态机恢复（轻量）
//
// 记住三句话：
//   1. async = 编译器把函数变成"可暂停、可恢复"的状态机，不绑定 OS 线程
//   2. .await = 跟调度器说"等 IO 的时候线程你拿去用，数据到了喊我"——线程不休眠
//   3. DMA + 硬件中断 = 数据搬运全程 CPU 零参与，异步模型和现代硬件天然匹配
