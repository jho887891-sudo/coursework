use lesson_01_agent_model::*;

// ============================================================
// 测试 5：输入关闭 + 有事实 → Finish
// 验证：input_closed=true 且 notes 非空时，返回 Finish
// ============================================================
#[test]
fn closed_input_with_fact_finishes() {
    let state = AgentState {
        goal: Some("总结会议".into()),
        notes: vec!["事实 A".into(), "事实 B".into()],
        input_closed: true,
        ..Default::default()
    };

    let action = choose_action(&state, 10).unwrap();
    assert!(
        matches!(action, Action::Finish(_)),
        "输入关闭且有事实时应该 Finish，实际得到: {:?}",
        action
    );
}

// ============================================================
// 测试 6：输入关闭 + 无事实 → Stop("insufficient_input")
// 验证：input_closed=true 但 notes 为空时，合法终止
// ============================================================
#[test]
fn closed_input_without_fact_stops() {
    let state = AgentState {
        goal: Some("总结会议".into()),
        notes: vec![], // 没有事实
        input_closed: true,
        ..Default::default()
    };

    let action = choose_action(&state, 10).unwrap();
    assert_eq!(
        action,
        Action::Stop("insufficient_input"),
        "输入关闭但没有事实时，应该 Stop"
    );
}

// ============================================================
// 测试 7：预算优先级高于继续处理
// 验证：turns >= max_turns 时，即使还有输入待处理、有事实，
//       也必须返回 Stop("turn_budget")
// ============================================================
#[test]
fn budget_has_priority_over_continuation() {
    // 这个状态表示：还有很多事可以做，但轮数已经用完了
    let state = AgentState {
        turns: 5,
        goal: Some("目标".into()),
        notes: vec!["还有很多事实".into()],
        input_closed: false, // 输入还没关闭，理论上应该继续
        completed: false,
        status: RunStatus::Running,
    };

    let action = choose_action(&state, 5).unwrap();
    assert_eq!(
        action,
        Action::Stop("turn_budget"),
        "预算耗尽时必须优先于继续处理"
    );
}

#[test]
fn no_more_input_without_goal_stops() {
    let state = AgentState {
        input_closed: true,
        ..Default::default()
    };

    assert_eq!(
        choose_action(&state, 5).unwrap(),
        Action::Stop("insufficient_goal")
    );
}

#[test]
fn finish_marks_state_completed() {
    let run = run_scripted_with_state(
        &[
            Observation::UserGoal("总结".into()),
            Observation::Fact("事实 A".into()),
            Observation::NoMoreInput,
        ],
        6,
    )
    .unwrap();

    assert!(run.final_state.completed);
    assert_eq!(run.final_state.status, RunStatus::Completed);
    assert!(matches!(run.actions.last(), Some(Action::Finish(_))));
}

// ============================================================
// 测试 8：scripted 循环永不超出预算
// 验证：大量 Observation + 小 max_turns → 循环终止且不超标
// ============================================================
#[test]
fn scripted_loop_never_exceeds_budget() {
    // 构造 20 个 Fact + 一个 NoMoreInput —— 远超过预算
    let mut observations: Vec<Observation> = (0..20)
        .map(|i| Observation::Fact(format!("事实 {}", i)))
        .collect();
    observations.push(Observation::NoMoreInput);

    let max_turns = 5;
    let actions = run_scripted(&observations, max_turns).unwrap();

    // 断言 1：必须终止（不会无限循环）
    assert!(!actions.is_empty(), "应该至少返回一个动作");

    // 断言 2：最后一个动作必须是 Finish 或 Stop
    let last = actions.last().unwrap();
    assert!(
        matches!(last, Action::Finish(_) | Action::Stop(_)),
        "循环必须以 Finish 或 Stop 结束，实际: {:?}",
        last
    );

    // 断言 3：处理轮数不能超过预算（+1 是因为可能有初始动作）
    assert!(
        actions.len() <= max_turns + 1,
        "动作数 {} 不应超过预算 {} + 1",
        actions.len(),
        max_turns
    );
}
