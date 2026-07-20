use lesson_01_agent_model::agent::{MeetingAction, MeetingAgent, MeetingObservation, MeetingState};
use lesson_01_agent_model::chatbot::ChatbotAssistant;
use lesson_01_agent_model::workflow::{MeetingWorkflow, WorkflowError};
use lesson_01_agent_model::MeetingRequest;
use std::path::PathBuf;

fn fixture_path() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test_material.txt")
        .to_string_lossy()
        .into_owned()
}

fn complete_request() -> MeetingRequest {
    MeetingRequest {
        topic: "Q3 项目评审".into(),
        date: Some("2026-08-01".into()),
        participants: vec!["张三".into(), "李四".into()],
        material_path: Some(fixture_path()),
    }
}

fn successful_read() -> MeetingObservation {
    MeetingObservation::MaterialLoaded {
        path: fixture_path(),
        content: "项目背景与待决策事项".into(),
    }
}

#[test]
fn scenario_1_information_complete_all_succeed() {
    let request = complete_request();

    assert!(ChatbotAssistant::respond(&request).contains("Q3 项目评审"));
    assert!(MeetingWorkflow::execute(&request).is_ok());

    let run = MeetingAgent::run(&request, &[successful_read()], 10).unwrap();
    assert!(matches!(
        run.actions.as_slice(),
        [
            MeetingAction::ReadMaterial { .. },
            MeetingAction::ProduceChecklist { .. }
        ]
    ));
    assert!(run.final_state.completed);
}

#[test]
fn scenario_2_missing_date_has_three_distinct_behaviors() {
    let request = MeetingRequest {
        date: None,
        ..complete_request()
    };

    // 这是刻意定义的 naive Chatbot baseline：它会做未经证实的补全。
    assert!(ChatbotAssistant::respond(&request).contains("自动推定"));
    assert_eq!(
        MeetingWorkflow::execute(&request).unwrap_err(),
        WorkflowError::MissingDate
    );

    let run = MeetingAgent::run(&request, &[], 10).unwrap();
    assert_eq!(run.actions, vec![MeetingAction::AskForDate]);
}

#[test]
fn scenario_3_read_failure_becomes_observation_then_recovery_action() {
    let request = MeetingRequest {
        material_path: Some("missing.txt".into()),
        ..complete_request()
    };

    assert!(ChatbotAssistant::respond(&request).contains("已根据"));
    assert!(matches!(
        MeetingWorkflow::execute(&request).unwrap_err(),
        WorkflowError::MaterialNotFound(_)
    ));

    let run = MeetingAgent::run(
        &request,
        &[MeetingObservation::MaterialReadFailed {
            path: "missing.txt".into(),
        }],
        10,
    )
    .unwrap();
    assert_eq!(
        run.actions,
        vec![
            MeetingAction::ReadMaterial {
                path: "missing.txt".into()
            },
            MeetingAction::AskForMaterialPath
        ]
    );
    assert!(run.final_state.material_path.is_none());
}

#[test]
fn scenario_4_user_refusal_is_explicit_termination() {
    let request = MeetingRequest {
        date: None,
        ..complete_request()
    };

    let run = MeetingAgent::run(&request, &[MeetingObservation::UserDeclined], 10).unwrap();
    assert_eq!(
        run.actions,
        vec![
            MeetingAction::AskForDate,
            MeetingAction::Stop {
                reason: "user_declined"
            }
        ]
    );
}

#[test]
fn scenario_5_complete_request_never_asks_for_known_information() {
    let run = MeetingAgent::run(&complete_request(), &[successful_read()], 10).unwrap();

    assert!(!run.actions.iter().any(|action| {
        matches!(
            action,
            MeetingAction::AskForTopic
                | MeetingAction::AskForDate
                | MeetingAction::AskForParticipants
                | MeetingAction::AskForMaterialPath
        )
    }));
}

#[test]
fn action_is_not_observation() {
    let state = MeetingState::from(&complete_request());
    let action = state.decide(10);

    assert!(matches!(action, MeetingAction::ReadMaterial { .. }));
    assert!(
        state.material_text.is_none(),
        "提出 ReadMaterial 不能凭空产生 MaterialLoaded"
    );
}

#[test]
fn unexpected_tool_result_is_rejected() {
    let mut state = MeetingState::from(&complete_request());
    assert!(state
        .apply(MeetingObservation::MaterialLoaded {
            path: "another.txt".into(),
            content: "wrong result".into(),
        })
        .is_err());
}

#[test]
fn observation_must_match_the_previous_action() {
    let request = MeetingRequest {
        date: None,
        ..complete_request()
    };

    let result = MeetingAgent::run(
        &request,
        &[MeetingObservation::UserProvidedParticipants(vec![
            "不匹配的反馈".into(),
        ])],
        10,
    );

    assert!(result.is_err());
}

#[test]
fn meeting_agent_has_a_hard_step_budget() {
    let request = MeetingRequest {
        topic: String::new(),
        date: None,
        participants: vec![],
        material_path: None,
    };

    let run = MeetingAgent::run(
        &request,
        &[
            MeetingObservation::UserProvidedTopic("评审会".into()),
            MeetingObservation::UserProvidedDate("2026-08-01".into()),
            MeetingObservation::UserProvidedParticipants(vec!["张三".into()]),
        ],
        2,
    )
    .unwrap();

    assert_eq!(
        run.actions.last(),
        Some(&MeetingAction::Stop {
            reason: "step_budget"
        })
    );
}
