pub mod agent;
pub mod chatbot;
pub mod workflow;

/// 通用 Agent 可以向环境提出的最小动作集合。
///
/// `Record` 表示“输出当前状态摘要并等待更多环境输入”，不是直接修改 State；
/// State 只能由 `apply_observation` 根据真实 Observation 更新。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    AskForGoal,
    Record(String),
    Finish(String),
    Stop(&'static str),
}

/// 终结状态：和 `completed: bool` 配合使用。
///
/// 为什么不用一个 `Option<&'static str>` 代替？
/// 因为 `completed` 在课程公开接口（acceptance.rs）中直接作为 bool 被断言，
/// 不能删除。`RunStatus` 补充更细粒度的终止原因：None=运行中，
/// Some("step_budget")=预算耗尽，Some("user_declined")=用户拒绝。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RunStatus {
    #[default]
    Running,
    Completed,
    Stopped(&'static str),
}

#[derive(Debug, Clone, Default)]
pub struct AgentState {
    /// 已成功处理的 Observation 数量。
    pub turns: usize,
    pub goal: Option<String>,
    pub notes: Vec<String>,
    /// 为兼容课程公开接口保留；真实终止语义以 `status` 为准。
    pub completed: bool,
    pub input_closed: bool,
    pub status: RunStatus,
}

impl AgentState {
    pub fn is_terminal(&self) -> bool {
        self.completed || !matches!(self.status, RunStatus::Running)
    }
}

#[derive(Debug, Clone)]
pub enum Observation {
    UserGoal(String),
    Fact(String),
    NoMoreInput,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LabError {
    NotImplemented(&'static str),
    InvalidTransition(&'static str),
}

/// 会议准备请求——Chatbot、Workflow 和 Agent 共用相同输入。
#[derive(Debug, Clone)]
pub struct MeetingRequest {
    pub topic: String,
    pub date: Option<String>,
    pub participants: Vec<String>,
    pub material_path: Option<String>,
}

/// 纯规则 Policy：只读取 State，不直接修改 State。
///
/// 优先级设计：
/// 1. 已终止（completed / status）→ Stop（不能再有任何动作）
/// 2. 预算耗尽（turns >= max_turns）→ Stop（硬预算，程序强制，不由 Policy 自觉）
/// 3. 输入已关闭（input_closed）→ 此时不能再向用户索取，只能 Finish 或 Stop
/// 4. 缺少目标                    → AskForGoal
/// 5. 正常继续                    → Record（等待更多环境输入）
///
/// 关键：`input_closed` 的检查必须在 `AskForGoal` 之前。
/// 如果反过来——先问目标再检查 input_closed——那用户说了"没有更多信息"之后，
/// Agent 还会继续索要目标，这违反了"尊重用户边界"的原则。
pub fn choose_action(state: &AgentState, max_turns: usize) -> Result<Action, LabError> {
    if state.completed || matches!(state.status, RunStatus::Completed) {
        return Ok(Action::Stop("already_completed"));
    }
    if let RunStatus::Stopped(reason) = state.status {
        return Ok(Action::Stop(reason));
    }
    if state.turns >= max_turns {
        return Ok(Action::Stop("turn_budget"));
    }

    // 环境已经关闭输入时，不能再向用户索取目标。
    if state.input_closed {
        if state.goal.is_none() {
            return Ok(Action::Stop("insufficient_goal"));
        }
        if state.notes.is_empty() {
            return Ok(Action::Stop("insufficient_input"));
        }
        return Ok(Action::Finish(state.notes.join("\n")));
    }

    if state.goal.is_none() {
        return Ok(Action::AskForGoal);
    }

    Ok(Action::Record(state.notes.join("\n")))
}

/// Reducer：只有真实环境 Observation 可以更新 State。
///
/// 两个前置守卫：
/// - `is_terminal()` → 已终止的状态不可再修改（completed / stopped 都是终态）
/// - `input_closed`   → 环境已关闭输入通道，拒绝所有新 Observation
///
/// 空字符串处理：`trim()` 后为空视为无效输入，返回 InvalidTransition 而非静默写入。
/// 这是防止"空内容污染 State"的最后一道防线。
pub fn apply_observation(state: &mut AgentState, observation: Observation) -> Result<(), LabError> {
    if state.is_terminal() {
        return Err(LabError::InvalidTransition("already completed"));
    }
    if state.input_closed {
        return Err(LabError::InvalidTransition("input already closed"));
    }

    match observation {
        Observation::UserGoal(text) => {
            let text = text.trim();
            if text.is_empty() {
                return Err(LabError::InvalidTransition("empty goal"));
            }
            state.goal = Some(text.to_owned());
        }
        Observation::Fact(text) => {
            let text = text.trim();
            if text.is_empty() {
                return Err(LabError::InvalidTransition("empty fact"));
            }
            state.notes.push(text.to_owned());
        }
        Observation::NoMoreInput => {
            state.input_closed = true;
        }
    }

    state.turns += 1;
    Ok(())
}

/// 把动作的"终止语义"同步回 State。
///
/// 为什么需要这个函数？
/// `choose_action` 返回 `Finish` 或 `Stop` 时，只是"表达了终止意图"。
/// 只有调用了本函数，State 才真正进入终态（completed=true 或 status=Stopped）。
/// 此后 `is_terminal()` 返回 true，所有后续操作被拒绝。
///
/// 这是"动作≠状态"原则的体现：动作是 Policy 的输出，状态是 Reducer 的结果。
fn apply_terminal_action(state: &mut AgentState, action: &Action) {
    match action {
        Action::Finish(_) => {
            state.completed = true;
            state.status = RunStatus::Completed;
        }
        Action::Stop(reason) => {
            state.status = RunStatus::Stopped(reason);
        }
        Action::AskForGoal | Action::Record(_) => {}
    }
}

#[derive(Debug, Clone)]
pub struct ScriptedRun {
    pub actions: Vec<Action>,
    pub final_state: AgentState,
}

/// 脚本化循环：先写入 Observation，再调用 Policy。
///
/// ## 和交互式 Agent Loop 的区别
///
/// 交互式 Loop：初始 State  → Policy → Action → Environment → Observation → …
/// 脚本化重放：Observation → State → Policy → Action → 终止？
///
/// 本函数是"脚本化重放"——Observation 切片代表**已经发生**的环境事实。
/// 因此必须先 apply_observation 写入 State，再 choose_action 选择下一个动作。
/// 不能像交互式那样"对空状态先做一次初始 Policy 调用"——那会产生一个多余的
/// AskForGoal，而实际上 UserGoal 已经在 observations[0] 里了。
///
/// ## 终止保证
///
/// 循环遇到 Finish 或 Stop 立即 break，因此返回的 actions 长度不会超过
/// observations.len()。`max_turns` 在 Policy 层提供第二重保障。
pub fn run_scripted_with_state(
    observations: &[Observation],
    max_turns: usize,
) -> Result<ScriptedRun, LabError> {
    let mut state = AgentState::default();
    let mut actions = Vec::new();

    if observations.is_empty() {
        let action = choose_action(&state, max_turns)?;
        apply_terminal_action(&mut state, &action);
        actions.push(action);
        return Ok(ScriptedRun {
            actions,
            final_state: state,
        });
    }

    for observation in observations {
        apply_observation(&mut state, observation.clone())?;
        let action = choose_action(&state, max_turns)?;
        let terminal = matches!(action, Action::Finish(_) | Action::Stop(_));
        apply_terminal_action(&mut state, &action);
        actions.push(action);
        if terminal {
            break;
        }
    }

    Ok(ScriptedRun {
        actions,
        final_state: state,
    })
}

/// 保持课程公开验收接口：只返回动作序列。
pub fn run_scripted(
    observations: &[Observation],
    max_turns: usize,
) -> Result<Vec<Action>, LabError> {
    Ok(run_scripted_with_state(observations, max_turns)?.actions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_compiles() {
        assert_eq!(
            choose_action(&AgentState::default(), 3),
            Ok(Action::AskForGoal)
        );
    }
}
