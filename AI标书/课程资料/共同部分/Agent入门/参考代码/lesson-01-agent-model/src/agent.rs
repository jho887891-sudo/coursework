use crate::{LabError, MeetingRequest};

/// 会议助手可以执行的动作。
///
/// 注意和通用 `Action`（lib.rs）的关系：
/// - 通用 Action 是一个"最小抽象"——只演示 Agent Loop 的结构
/// - MeetingAction 是一个"具体领域动作"——每个变体对应一个明确的业务语义
/// - 两者都是 Action，只是抽象层级不同
///
/// 终止动作：ProduceChecklist（正常完成）、Stop（异常终止）
/// 询问动作：AskForTopic / AskForDate / AskForParticipants / AskForMaterialPath
/// 工具动作：ReadMaterial（需要环境返回 Observation 才能继续）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeetingAction {
    AskForTopic,
    AskForDate,
    AskForParticipants,
    AskForMaterialPath,
    ReadMaterial { path: String },
    ProduceChecklist { summary: String },
    Stop { reason: &'static str },
}

/// 环境返回的观察——只有这些才能写入 MeetingState。
///
/// ## Action 和 Observation 的配对关系
///
/// 这是 Lesson 1 最核心的架构原则：
/// - Action 是"我想做什么"（意图）
/// - Observation 是"实际发生了什么"（事实）
/// - 只有 Observation 能修改 State，Action 不能
///
/// 配对规则（由 `observation_matches_action` 强制）：
///   AskForTopic       → UserProvidedTopic
///   AskForDate        → UserProvidedDate
///   AskForParticipants → UserProvidedParticipants
///   AskForMaterialPath → UserProvidedMaterialPath
///   ReadMaterial      → MaterialLoaded 或 MaterialReadFailed
///   任何询问动作       → UserDeclined（用户拒绝回答）
///
/// 不匹配的 Observation（如 UserProvidedDate 回应 AskForTopic）会被拒绝。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeetingObservation {
    UserProvidedTopic(String),
    UserProvidedDate(String),
    UserProvidedParticipants(Vec<String>),
    UserProvidedMaterialPath(String),
    MaterialLoaded { path: String, content: String },
    MaterialReadFailed { path: String },
    UserDeclined,
}

/// 会议助手的内部状态——Agent 的"记事本"。
///
/// ## 和通用 `AgentState`（lib.rs）的关系
///
/// 通用 AgentState 用 `goal: Option<String>` + `notes: Vec<String>` 存储一切；
/// MeetingState 把信息拆成结构化字段（topic / date / participants / …）。
/// 这是从"通用模型"到"领域模型"的自然演进——真实 Agent 需要结构化的 State。
///
/// ## 关键字段
/// - `material_path` vs `material_text`：路径只是引用，内容是实际读取结果。
///   路径存在 ≠ 读取成功。只有 MaterialLoaded 能写入 material_text。
/// - `user_declined`：用户拒绝补充信息是合法终止条件，不是错误。
/// - `steps`：每调用一次 decide() 就 +1，不受 Observation 数量影响。
#[derive(Debug, Clone, Default)]
pub struct MeetingState {
    pub topic: Option<String>,
    pub date: Option<String>,
    pub participants: Vec<String>,
    /// 材料路径——可能来自用户输入或 Agent 请求后的返回。
    /// 读取失败时此字段被清空，下一轮 Policy 会重新询问。
    pub material_path: Option<String>,
    /// 材料内容——只有 MaterialLoaded Observation 能写入。
    /// 和 material_path 分离：路径是引用，内容是数据。
    pub material_text: Option<String>,
    /// 用户明确拒绝继续补充信息。
    pub user_declined: bool,
    /// decide() 调用次数，用于硬预算。
    pub steps: usize,
    /// ProduceChecklist 之后置 true，状态不可再修改。
    pub completed: bool,
}

/// 从 MeetingRequest 创建初始 State。
///
/// 所有字符串字段自动 trim，空字符串视为"未提供"（None 或 空 Vec）。
/// 这保证了"空格"="未填写"，不会让空白输入混入 State。
impl From<&MeetingRequest> for MeetingState {
    fn from(request: &MeetingRequest) -> Self {
        let topic = (!request.topic.trim().is_empty()).then(|| request.topic.trim().to_owned());
        let date = request
            .date
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let participants = request
            .participants
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect();
        let material_path = request
            .material_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);

        Self {
            topic,
            date,
            participants,
            material_path,
            ..Self::default()
        }
    }
}

impl MeetingState {
    /// 会议领域的 Policy：根据当前 State 选择下一个 Action。
    ///
    /// ## 优先级（命中即返回）
    ///
    /// 1. completed              → Stop("already_completed")
    /// 2. steps >= max_steps     → Stop("step_budget")     ← 硬预算，不依赖 Policy 自觉
    /// 3. user_declined          → Stop("user_declined")   ← 用户拒绝是合法终止
    /// 4. topic 为空             → AskForTopic
    /// 5. date 为空              → AskForDate
    /// 6. participants 为空      → AskForParticipants
    /// 7. material_path 为空     → AskForMaterialPath
    /// 8. material_text 为空     → ReadMaterial            ← 有路径但没读 → 工具动作
    /// 9. 全部齐全               → ProduceChecklist        ← 正常完成
    ///
    /// ## 和通用 choose_action 的区别
    ///
    /// 通用版：input_closed 后不能再索取 → 用 Stop/Finish 收束。
    /// 会议版：没有 input_closed 概念——交互是逐个 Observation 驱动的，
    /// Policy 只管"当前缺少什么就请求什么"。
    pub fn decide(&self, max_steps: usize) -> MeetingAction {
        if self.completed {
            return MeetingAction::Stop {
                reason: "already_completed",
            };
        }
        if self.steps >= max_steps {
            return MeetingAction::Stop {
                reason: "step_budget",
            };
        }
        if self.user_declined {
            return MeetingAction::Stop {
                reason: "user_declined",
            };
        }
        if self.topic.is_none() {
            return MeetingAction::AskForTopic;
        }
        if self.date.is_none() {
            return MeetingAction::AskForDate;
        }
        if self.participants.is_empty() {
            return MeetingAction::AskForParticipants;
        }
        let Some(path) = &self.material_path else {
            return MeetingAction::AskForMaterialPath;
        };
        if self.material_text.is_none() {
            return MeetingAction::ReadMaterial { path: path.clone() };
        }

        MeetingAction::ProduceChecklist {
            summary: format!(
                "主题：{}\n日期：{}\n参与人：{}\n材料摘要：{}",
                self.topic.as_deref().unwrap_or_default(),
                self.date.as_deref().unwrap_or_default(),
                self.participants.join("、"),
                self.material_text.as_deref().unwrap_or_default()
            ),
        }
    }

    /// 根据环境 Observation 更新 State——Agent 的 Reducer。
    ///
    /// ## 关键设计决策
    ///
    /// ### MaterialLoaded：路径必须匹配
    /// 只有 `observation.path == state.material_path` 时才能写入 material_text。
    /// 这防止了"请求路径 A 却收到路径 B 的内容"——Observation 必须与 Action 对应。
    ///
    /// ### MaterialReadFailed：清除无效路径
    /// 文件不存在时，将 material_path 重置为 None 而非保留失效值。
    /// 下一轮 decide() 会再次返回 AskForMaterialPath，形成"失败 → 重问"循环。
    /// 这正是 Agent 优于 Workflow 的地方：Workflow 直接报错终止，Agent 能恢复。
    ///
    /// ### UserDeclined：不是错误
    /// 用户拒绝是合法终止条件，写入 user_declined 而非返回 Err。
    /// Policy 负责在下一轮 decide() 中转换为 Stop。
    ///
    /// ### 前置守卫
    /// completed 后拒绝一切 observation——终态不可修改。
    pub fn apply(&mut self, observation: MeetingObservation) -> Result<(), LabError> {
        if self.completed {
            return Err(LabError::InvalidTransition("already completed"));
        }

        match observation {
            MeetingObservation::UserProvidedTopic(value) => {
                self.topic = Some(non_empty(value, "empty topic")?);
            }
            MeetingObservation::UserProvidedDate(value) => {
                self.date = Some(non_empty(value, "empty date")?);
            }
            MeetingObservation::UserProvidedParticipants(values) => {
                let cleaned: Vec<String> = values
                    .into_iter()
                    .map(|value| value.trim().to_owned())
                    .filter(|value| !value.is_empty())
                    .collect();
                if cleaned.is_empty() {
                    return Err(LabError::InvalidTransition("empty participants"));
                }
                self.participants = cleaned;
            }
            MeetingObservation::UserProvidedMaterialPath(value) => {
                self.material_path = Some(non_empty(value, "empty material path")?);
                self.material_text = None;
            }
            MeetingObservation::MaterialLoaded { path, content } => {
                let expected = self
                    .material_path
                    .as_deref()
                    .ok_or(LabError::InvalidTransition("no material requested"))?;
                if expected != path {
                    return Err(LabError::InvalidTransition("unexpected material path"));
                }
                self.material_text = Some(non_empty(content, "empty material")?);
            }
            MeetingObservation::MaterialReadFailed { path } => {
                let expected = self
                    .material_path
                    .as_deref()
                    .ok_or(LabError::InvalidTransition("no material requested"))?;
                if expected != path {
                    return Err(LabError::InvalidTransition("unexpected material path"));
                }
                // 失败是 Observation：清除无效路径，下一轮 Policy 会请求新路径。
                self.material_path = None;
                self.material_text = None;
            }
            MeetingObservation::UserDeclined => {
                self.user_declined = true;
            }
        }
        Ok(())
    }
}

/// trim 后非空 → Ok；空字符串 → Err。
/// 和通用版 apply_observation 的空值检测保持一致策略。
fn non_empty(value: String, reason: &'static str) -> Result<String, LabError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(LabError::InvalidTransition(reason));
    }
    Ok(value.to_owned())
}

#[derive(Debug, Clone)]
pub struct MeetingRun {
    pub actions: Vec<MeetingAction>,
    pub final_state: MeetingState,
}

pub struct MeetingAgent;

impl MeetingAgent {
    /// 会议 Agent 的脚本化运行。
    ///
    /// ## 循环结构（和通用 run_scripted 不同）
    ///
    /// 通用版：for observation in observations { apply → choose }
    /// 本版：  loop { decide → 消费一个 observation → apply → 继续 }
    ///
    /// 区别在于：通用版"先 apply 再 choose"，本版"先 decide 再 consume"。
    /// 这反映了交互式 Agent 的真实模型：Policy 先于 Observation——
    /// Agent 先决定"我需要什么信息"，然后环境才返回 Observation。
    ///
    /// ## Action-Observation 配对
    ///
    /// 每个 Action 只消费一个 Observation，且必须配对（observation_matches_action）。
    /// 这是对真实 Agent 的模拟：你做 ReadMaterial，环境必须返回 MaterialLoaded
    /// 或 MaterialReadFailed，不能返回 UserProvidedDate。
    ///
    /// ## 终止条件
    /// - ProduceChecklist → 正常完成
    /// - Stop → 异常/合法终止
    /// - Observation 耗尽 → 等待用户/工具（不是错误，是"等待中"）
    /// - max_steps 预算耗尽 → Policy 层返回 Stop("step_budget")
    pub fn run(
        request: &MeetingRequest,
        observations: &[MeetingObservation],
        max_steps: usize,
    ) -> Result<MeetingRun, LabError> {
        let mut state = MeetingState::from(request);
        let mut actions = Vec::new();
        let mut observations = observations.iter().cloned();

        loop {
            let action = state.decide(max_steps);
            state.steps += 1;
            let terminal = matches!(
                action,
                MeetingAction::ProduceChecklist { .. } | MeetingAction::Stop { .. }
            );
            if matches!(action, MeetingAction::ProduceChecklist { .. }) {
                state.completed = true;
            }
            actions.push(action);

            if terminal {
                break;
            }

            let Some(observation) = observations.next() else {
                break;
            };
            if !observation_matches_action(
                actions.last().expect("action was just pushed"),
                &observation,
            ) {
                return Err(LabError::InvalidTransition(
                    "unexpected observation for action",
                ));
            }
            state.apply(observation)?;
        }

        Ok(MeetingRun {
            actions,
            final_state: state,
        })
    }
}

/// Action-Observation 配对校验。
///
/// 这是 Lesson 1 讲义第 5 节的核心教条：
/// "不要把'计划读取'和'已经读到'混为一谈"。
///
/// 本函数确保只有匹配的 Action-Observation 对才能通过。
/// 例如：
///   AskForDate + UserProvidedDate         → ✅ 匹配
///   AskForDate + UserProvidedParticipants → ❌ 不匹配 → 返回 Err
///   ReadMaterial + MaterialLoaded         → ✅ 匹配
///   ReadMaterial + UserDeclined           → ❌ ReadMaterial 不能直接收到拒绝
///
/// UserDeclined 比较特殊：它可以配合任何询问动作（AskFor*），
/// 但不能配合 ReadMaterial——工具动作不存在"用户拒绝"。
fn observation_matches_action(action: &MeetingAction, observation: &MeetingObservation) -> bool {
    if matches!(observation, MeetingObservation::UserDeclined) {
        return matches!(
            action,
            MeetingAction::AskForTopic
                | MeetingAction::AskForDate
                | MeetingAction::AskForParticipants
                | MeetingAction::AskForMaterialPath
        );
    }

    matches!(
        (action, observation),
        (
            MeetingAction::AskForTopic,
            MeetingObservation::UserProvidedTopic(_)
        ) | (
            MeetingAction::AskForDate,
            MeetingObservation::UserProvidedDate(_)
        ) | (
            MeetingAction::AskForParticipants,
            MeetingObservation::UserProvidedParticipants(_)
        ) | (
            MeetingAction::AskForMaterialPath,
            MeetingObservation::UserProvidedMaterialPath(_)
        ) | (
            MeetingAction::ReadMaterial { .. },
            MeetingObservation::MaterialLoaded { .. }
        ) | (
            MeetingAction::ReadMaterial { .. },
            MeetingObservation::MaterialReadFailed { .. }
        )
    )
}
