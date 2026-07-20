use lesson_01_agent_model::*;

// ============================================================
// 测试 1：空目标被拒绝
// 验证：UserGoal("") 不能静默写入，必须返回错误
// ============================================================
#[test]
fn empty_goal_is_rejected() {
    let mut state = AgentState::default();
    let result = apply_observation(&mut state, Observation::UserGoal("".into()));
    assert!(result.is_err(), "空目标应该被拒绝，但成功了");
}

#[test]
fn whitespace_goal_and_fact_are_rejected() {
    let mut state = AgentState::default();
    assert_eq!(
        apply_observation(&mut state, Observation::UserGoal("   ".into())),
        Err(LabError::InvalidTransition("empty goal"))
    );
    assert_eq!(
        apply_observation(&mut state, Observation::Fact("\n\t".into())),
        Err(LabError::InvalidTransition("empty fact"))
    );
    assert_eq!(state.turns, 0);
}

// ============================================================
// 测试 2：事实按顺序追加
// 验证：连续两个 Fact 后，notes 保持插入顺序
// ============================================================
#[test]
fn fact_is_appended_in_order() {
    let mut state = AgentState::default();

    apply_observation(&mut state, Observation::Fact("第一条事实".into())).unwrap();
    apply_observation(&mut state, Observation::Fact("第二条事实".into())).unwrap();

    assert_eq!(
        state.notes,
        vec!["第一条事实", "第二条事实"],
        "事实应该按接收顺序存储在 notes 中"
    );
}

// ============================================================
// 测试 3：NoMoreInput 关闭输入
// 验证：收到 NoMoreInput 后 input_closed 变为 true
// ============================================================
#[test]
fn no_more_input_closes_input() {
    let mut state = AgentState::default();

    // 初始状态 input_closed 应为 false
    assert!(!state.input_closed);

    apply_observation(&mut state, Observation::NoMoreInput).unwrap();

    // 处理后 input_closed 应为 true
    assert!(
        state.input_closed,
        "NoMoreInput 应该将 input_closed 设为 true"
    );
}

// ============================================================
// 测试 4：已完成状态拒绝所有 Observation
// 验证：completed == true 时，UserGoal / Fact / NoMoreInput 全部被拒绝
// ============================================================
#[test]
fn completed_state_rejects_every_observation() {
    // --- UserGoal ---
    let mut state = AgentState {
        completed: true,
        ..Default::default()
    };
    let r1 = apply_observation(&mut state, Observation::UserGoal("目标".into()));
    assert!(r1.is_err(), "completed 状态下 UserGoal 应该被拒绝");

    // --- Fact ---
    let mut state = AgentState {
        completed: true,
        ..Default::default()
    };
    let r2 = apply_observation(&mut state, Observation::Fact("事实".into()));
    assert!(r2.is_err(), "completed 状态下 Fact 应该被拒绝");

    // --- NoMoreInput ---
    let mut state = AgentState {
        completed: true,
        ..Default::default()
    };
    let r3 = apply_observation(&mut state, Observation::NoMoreInput);
    assert!(r3.is_err(), "completed 状态下 NoMoreInput 应该被拒绝");
}

#[test]
fn observation_after_input_closed_is_rejected() {
    let mut state = AgentState::default();
    apply_observation(&mut state, Observation::NoMoreInput).unwrap();

    assert_eq!(
        apply_observation(&mut state, Observation::Fact("late fact".into())),
        Err(LabError::InvalidTransition("input already closed"))
    );
    assert_eq!(
        apply_observation(&mut state, Observation::UserGoal("late goal".into())),
        Err(LabError::InvalidTransition("input already closed"))
    );
}
