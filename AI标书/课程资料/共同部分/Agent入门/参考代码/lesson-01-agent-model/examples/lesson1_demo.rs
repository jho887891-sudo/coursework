use lesson_01_agent_model::agent::{MeetingAction, MeetingObservation, MeetingState};
use lesson_01_agent_model::MeetingRequest;
use std::env;
use std::io::{self, Write};

const MAX_STEPS: usize = 10;

#[derive(Clone, Copy)]
enum Scenario {
    Complete,
    MissingDate,
    ReadFailure,
    UserDeclined,
}

impl Scenario {
    fn from_arg(value: &str) -> Option<Self> {
        match value {
            "complete" => Some(Self::Complete),
            "missing-date" => Some(Self::MissingDate),
            "read-failure" => Some(Self::ReadFailure),
            "user-declined" => Some(Self::UserDeclined),
            _ => None,
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let pause_enabled = !args.iter().any(|arg| arg == "--no-pause");
    let selected = args
        .iter()
        .find(|arg| !arg.starts_with("--"))
        .map(String::as_str);

    print_course_header();

    match selected {
        Some("all") => {
            for scenario in [
                Scenario::Complete,
                Scenario::MissingDate,
                Scenario::ReadFailure,
                Scenario::UserDeclined,
            ] {
                run_scenario(scenario, pause_enabled);
            }
        }
        Some(value) => match Scenario::from_arg(value) {
            Some(scenario) => run_scenario(scenario, pause_enabled),
            None => {
                eprintln!("未知场景：{value}");
                print_usage();
                std::process::exit(2);
            }
        },
        None => run_menu(pause_enabled),
    }
}

fn run_menu(pause_enabled: bool) {
    loop {
        println!();
        println!("请选择课堂演示场景：");
        println!("  1. 信息完整：路径存在不等于材料已读取");
        println!("  2. 缺少日期：Agent 如何获取缺失信息");
        println!("  3. 读取失败：Observation 如何改变下一动作");
        println!("  4. 用户拒绝：明确、合法地终止");
        println!("  5. 顺序演示全部场景");
        println!("  q. 退出");
        print!("输入选择：");
        flush_stdout();

        let input = read_line();
        match input.trim() {
            "1" => run_scenario(Scenario::Complete, pause_enabled),
            "2" => run_scenario(Scenario::MissingDate, pause_enabled),
            "3" => run_scenario(Scenario::ReadFailure, pause_enabled),
            "4" => run_scenario(Scenario::UserDeclined, pause_enabled),
            "5" => {
                for scenario in [
                    Scenario::Complete,
                    Scenario::MissingDate,
                    Scenario::ReadFailure,
                    Scenario::UserDeclined,
                ] {
                    run_scenario(scenario, pause_enabled);
                }
            }
            "q" | "Q" => break,
            _ => println!("无法识别，请输入 1～5 或 q。"),
        }
    }
}

fn run_scenario(scenario: Scenario, pause_enabled: bool) {
    match scenario {
        Scenario::Complete => demo_complete(pause_enabled),
        Scenario::MissingDate => demo_missing_date(pause_enabled),
        Scenario::ReadFailure => demo_read_failure(pause_enabled),
        Scenario::UserDeclined => demo_user_declined(pause_enabled),
    }

    println!();
    println!("本场景演示结束。");
    separator();
}

fn demo_complete(pause_enabled: bool) {
    scenario_header(
        "场景 1：信息完整，但材料内容尚未读取",
        "教学目标：区分“拥有路径”“提出读取动作”和“真实读取成功”。",
    );

    let request = complete_request("test_material.txt");
    let mut state = MeetingState::from(&request);

    show_state(&state);
    show_allowed_actions();
    predict(
        pause_enabled,
        "现在应该直接 ProduceChecklist，还是先 ReadMaterial？",
    );

    let action = reveal_policy(&state);
    explain_action(&action);
    record_non_terminal_step(&mut state, &action);

    let observation = MeetingObservation::MaterialLoaded {
        path: "test_material.txt".into(),
        content: "项目背景：需要决定是否进入下一阶段。".into(),
    };
    reveal_observation(
        pause_enabled,
        &observation,
        "工具真正返回了非空材料内容，这时才能更新 material_text。",
    );
    state
        .apply(observation)
        .expect("scripted observation is valid");

    show_updated_state(&state);
    predict(pause_enabled, "所有必需信息已经齐全，下一动作是什么？");
    let action = reveal_policy(&state);
    explain_action(&action);
    finish_action(&mut state, &action);
    show_termination(&state, &action);
}

fn demo_missing_date(pause_enabled: bool) {
    scenario_header(
        "场景 2：缺少会议日期",
        "教学目标：观察 Agent 如何依据 State 选择一个信息获取动作。",
    );

    let request = MeetingRequest {
        date: None,
        ..complete_request("test_material.txt")
    };
    let mut state = MeetingState::from(&request);

    show_state(&state);
    show_allowed_actions();
    predict(
        pause_enabled,
        "Chatbot 可能猜日期，Workflow 会报错；Agent 应选择哪个 Action？",
    );

    let action = reveal_policy(&state);
    explain_action(&action);
    record_non_terminal_step(&mut state, &action);

    let observation = MeetingObservation::UserProvidedDate("2026-08-01".into());
    reveal_observation(
        pause_enabled,
        &observation,
        "日期来自用户回复，不是模型自行补全。",
    );
    state
        .apply(observation)
        .expect("scripted observation is valid");

    show_updated_state(&state);
    predict(
        pause_enabled,
        "日期已经补齐，但材料内容仍缺失，下一动作是什么？",
    );
    let action = reveal_policy(&state);
    explain_action(&action);
    record_non_terminal_step(&mut state, &action);

    let observation = MeetingObservation::MaterialLoaded {
        path: "test_material.txt".into(),
        content: "项目背景与会议议题。".into(),
    };
    reveal_observation(
        pause_enabled,
        &observation,
        "读取成功后，State 才拥有材料内容。",
    );
    state
        .apply(observation)
        .expect("scripted observation is valid");

    show_updated_state(&state);
    let action = reveal_policy(&state);
    finish_action(&mut state, &action);
    show_termination(&state, &action);
    show_architecture_comparison(
        "缺少日期",
        "naive Chatbot：自动推定日期",
        "Workflow：MissingDate",
        "Agent：AskForDate → 依据 UserProvidedDate 继续",
    );
}

fn demo_read_failure(pause_enabled: bool) {
    scenario_header(
        "场景 3：材料读取失败",
        "教学目标：错误也是 Observation；它应改变 State 和下一动作。",
    );

    let request = complete_request("missing.txt");
    let mut state = MeetingState::from(&request);

    show_state(&state);
    predict(pause_enabled, "当前有路径但没有内容，下一动作是什么？");
    let action = reveal_policy(&state);
    explain_action(&action);
    record_non_terminal_step(&mut state, &action);

    let observation = MeetingObservation::MaterialReadFailed {
        path: "missing.txt".into(),
    };
    reveal_observation(
        pause_enabled,
        &observation,
        "失败不应 panic，也不能伪装成功；Reducer 会清除无效路径。",
    );
    state
        .apply(observation)
        .expect("scripted observation is valid");

    show_updated_state(&state);
    predict(
        pause_enabled,
        "无效路径已被清除。是继续输出，还是请求一个新路径？",
    );
    let action = reveal_policy(&state);
    explain_action(&action);
    record_non_terminal_step(&mut state, &action);

    let observation = MeetingObservation::UserProvidedMaterialPath("replacement.txt".into());
    reveal_observation(
        pause_enabled,
        &observation,
        "用户提供了新路径，但此时仍然没有材料内容。",
    );
    state
        .apply(observation)
        .expect("scripted observation is valid");

    show_updated_state(&state);
    predict(pause_enabled, "有了新路径，下一动作是什么？");
    let action = reveal_policy(&state);
    explain_action(&action);
    record_non_terminal_step(&mut state, &action);

    let observation = MeetingObservation::MaterialLoaded {
        path: "replacement.txt".into(),
        content: "替代材料读取成功。".into(),
    };
    reveal_observation(
        pause_enabled,
        &observation,
        "只有匹配当前请求路径的工具结果才会被接受。",
    );
    state
        .apply(observation)
        .expect("scripted observation is valid");

    show_updated_state(&state);
    let action = reveal_policy(&state);
    finish_action(&mut state, &action);
    show_termination(&state, &action);
}

fn demo_user_declined(pause_enabled: bool) {
    scenario_header(
        "场景 4：用户拒绝补充日期",
        "教学目标：停止可以是正确结果，终止必须有明确原因。",
    );

    let request = MeetingRequest {
        date: None,
        ..complete_request("test_material.txt")
    };
    let mut state = MeetingState::from(&request);

    show_state(&state);
    predict(pause_enabled, "缺少日期时，Agent 首先应该做什么？");
    let action = reveal_policy(&state);
    explain_action(&action);
    record_non_terminal_step(&mut state, &action);

    let observation = MeetingObservation::UserDeclined;
    reveal_observation(
        pause_enabled,
        &observation,
        "用户明确拒绝后，Agent 不得猜测日期或无限重复询问。",
    );
    state
        .apply(observation)
        .expect("scripted observation is valid");

    show_updated_state(&state);
    predict(pause_enabled, "此时应该继续追问、自动猜测，还是明确停止？");
    let action = reveal_policy(&state);
    explain_action(&action);
    show_termination(&state, &action);
}

fn complete_request(path: &str) -> MeetingRequest {
    MeetingRequest {
        topic: "Q3 项目评审".into(),
        date: Some("2026-08-01".into()),
        participants: vec!["张三".into(), "李四".into()],
        material_path: Some(path.into()),
    }
}

fn print_course_header() {
    separator();
    println!("Lesson 1 课堂交互 Demo");
    println!("主题：State → Policy → Action → Observation → State");
    println!("规则：每一步先让学生预测，再按 Enter 揭晓。");
    separator();
}

fn scenario_header(title: &str, objective: &str) {
    println!();
    separator();
    println!("{title}");
    println!("{objective}");
    separator();
}

fn show_state(state: &MeetingState) {
    println!();
    println!("[当前 State]");
    println!("  topic          = {}", option_text(&state.topic));
    println!("  date           = {}", option_text(&state.date));
    println!(
        "  participants   = {}",
        if state.participants.is_empty() {
            "None".to_owned()
        } else {
            format!("Some({})", state.participants.join("、"))
        }
    );
    println!("  material_path  = {}", option_text(&state.material_path));
    println!("  material_text  = {}", option_text(&state.material_text));
    println!("  user_declined  = {}", state.user_declined);
    println!("  steps          = {}", state.steps);
    println!("  completed      = {}", state.completed);
}

fn show_updated_state(state: &MeetingState) {
    println!();
    println!("[Observation 写回后的 State]");
    show_state(state);
}

fn show_allowed_actions() {
    println!();
    println!("[允许的 Action]");
    println!("  AskForTopic");
    println!("  AskForDate");
    println!("  AskForParticipants");
    println!("  AskForMaterialPath");
    println!("  ReadMaterial");
    println!("  ProduceChecklist");
    println!("  Stop");
}

fn reveal_policy(state: &MeetingState) -> MeetingAction {
    let action = state.decide(MAX_STEPS);
    println!();
    println!("[Policy 选择]");
    println!("  {action:#?}");
    action
}

fn reveal_observation(pause_enabled: bool, observation: &MeetingObservation, explanation: &str) {
    pause(
        pause_enabled,
        "Action 只是意图。请预测 Environment 会返回什么，然后按 Enter……",
    );
    println!();
    println!("[Environment 返回 Observation]");
    println!("  {observation:#?}");
    println!();
    println!("[讲解]");
    println!("  {explanation}");
}

fn explain_action(action: &MeetingAction) {
    println!();
    println!("[为什么]");
    match action {
        MeetingAction::AskForTopic => println!("  topic 缺失，不能凭空生成会议主题。"),
        MeetingAction::AskForDate => println!("  date 缺失，必须从用户获取，不能自行猜测。"),
        MeetingAction::AskForParticipants => {
            println!("  participants 为空，需要获得参会人信息。")
        }
        MeetingAction::AskForMaterialPath => {
            println!("  当前没有有效材料路径，必须请求新的路径。")
        }
        MeetingAction::ReadMaterial { .. } => {
            println!("  当前只有路径，material_text 仍为 None；必须向环境发出读取动作。")
        }
        MeetingAction::ProduceChecklist { .. } => {
            println!("  主题、日期、参与人和真实材料内容均已具备，可以完成目标。")
        }
        MeetingAction::Stop { reason } => {
            println!("  Runtime 给出明确终止原因：{reason}。")
        }
    }
}

fn show_termination(state: &MeetingState, action: &MeetingAction) {
    println!();
    println!("[Termination]");
    match action {
        MeetingAction::ProduceChecklist { summary } => {
            println!("  Completed");
            println!("  原因：任务所需信息已经完整。");
            println!();
            println!("[最终输出摘要]");
            println!("{summary}");
        }
        MeetingAction::Stop { reason } => {
            println!("  Stopped");
            println!("  reason = {reason}");
        }
        _ => println!("  尚未终止。"),
    }
    println!("  final_state.completed = {}", state.completed);
}

fn show_architecture_comparison(scenario: &str, chatbot: &str, workflow: &str, agent: &str) {
    println!();
    println!("[三种架构对比：{scenario}]");
    println!("  {chatbot}");
    println!("  {workflow}");
    println!("  {agent}");
    println!();
    println!("结论：运行时需要获取缺失信息时，Agent 才体现出区别。");
}

fn record_non_terminal_step(state: &mut MeetingState, action: &MeetingAction) {
    if !matches!(
        action,
        MeetingAction::ProduceChecklist { .. } | MeetingAction::Stop { .. }
    ) {
        state.steps += 1;
    }
}

fn finish_action(state: &mut MeetingState, action: &MeetingAction) {
    if matches!(action, MeetingAction::ProduceChecklist { .. }) {
        state.completed = true;
    }
}

fn predict(pause_enabled: bool, question: &str) {
    println!();
    println!("[请学生预测]");
    println!("  {question}");
    pause(pause_enabled, "讨论后按 Enter 揭晓 Policy 的选择……");
}

fn pause(enabled: bool, prompt: &str) {
    if !enabled {
        return;
    }
    print!("{prompt}");
    flush_stdout();
    let _ = read_line();
}

fn option_text(value: &Option<String>) -> String {
    match value {
        Some(value) => format!("Some({value})"),
        None => "None".to_owned(),
    }
}

fn separator() {
    println!("============================================================");
}

fn print_usage() {
    eprintln!("用法：");
    eprintln!("  cargo run -p lesson-01-agent-model --example lesson1_demo");
    eprintln!("  cargo run -p lesson-01-agent-model --example lesson1_demo -- complete");
    eprintln!("  cargo run -p lesson-01-agent-model --example lesson1_demo -- missing-date");
    eprintln!("  cargo run -p lesson-01-agent-model --example lesson1_demo -- read-failure");
    eprintln!("  cargo run -p lesson-01-agent-model --example lesson1_demo -- user-declined");
    eprintln!("  cargo run -p lesson-01-agent-model --example lesson1_demo -- all --no-pause");
}

fn read_line() -> String {
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("failed to read stdin");
    input
}

fn flush_stdout() {
    io::stdout().flush().expect("failed to flush stdout");
}
