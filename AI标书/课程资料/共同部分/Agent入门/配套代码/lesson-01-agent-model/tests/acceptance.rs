use lesson_01_agent_model::*;

#[test]
#[ignore = "implement Lesson 1 basic policy"]
fn asks_when_goal_is_unknown() {
    assert_eq!(
        choose_action(&AgentState::default(), 3).unwrap(),
        Action::AskForGoal
    );
}

#[test]
#[ignore = "implement Lesson 1 state update"]
fn observation_updates_state() {
    let mut s = AgentState::default();
    apply_observation(&mut s, Observation::UserGoal("总结材料".into())).unwrap();
    assert_eq!(s.goal.as_deref(), Some("总结材料"));
}

#[test]
#[ignore = "implement Lesson 1 termination"]
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
