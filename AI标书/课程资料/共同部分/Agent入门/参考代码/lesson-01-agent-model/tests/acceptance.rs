use lesson_01_agent_model::*;

#[test]
#[ignore = "implement Lesson 1 basic policy"]
//测试当目标未知时，智能体应该询问用户明确的目标
fn asks_when_goal_is_unknown() {
    assert_eq!(
        choose_action(&AgentState::default(), 3).unwrap(),
        Action::AskForGoal
    );
}

#[test]
#[ignore = "implement Lesson 1 state update"]
//测试当智能体接收到一个Observation::UserGoal时，应该更新其状态中的goal字段
fn observation_updates_state() {
    let mut s = AgentState::default();
    apply_observation(&mut s, Observation::UserGoal("总结材料".into())).unwrap();
    assert_eq!(s.goal.as_deref(), Some("总结材料"));
}

#[test]
#[ignore = "implement Lesson 1 termination"]
//测试当智能体的turns达到max_turns时，应该返回Action::Stop
fn hard_budget_always_stops() {
    let s = AgentState {
        turns: 3,
        goal: Some("x".into()),
        ..Default::default()
    };
    assert_eq!(choose_action(&s, 3).unwrap(), Action::Stop("turn_budget"));
}

#[test]
#[ignore = "implement the complete loop"]
//测试当智能体接收到一系列Observation时，应该能够正确地执行动作并最终输出Action::Finish
fn scripted_loop_reaches_finish() {
    let actions = run_scripted(
        &[
            Observation::UserGoal("总结".into()),
            Observation::Fact("事实 A".into()),
            Observation::NoMoreInput,
        ],
        6,
    )
    .unwrap();
    assert!(matches!(actions.last(), Some(Action::Finish(_))));
}

#[test]
#[ignore = "reject observations after termination"]
//测试当智能体已经完成任务后，应该拒绝接收新的Observation
fn completed_state_cannot_accept_more_observations() {
    let mut state = AgentState {
        completed: true,
        ..Default::default()
    };
    assert_eq!(
        apply_observation(&mut state, Observation::Fact("late fact".into())),
        Err(LabError::InvalidTransition("already completed"))
    );
}
