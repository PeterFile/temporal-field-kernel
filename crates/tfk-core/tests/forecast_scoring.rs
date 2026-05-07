use tfk_core::ForecastScorer;
use tfk_protocol::{CandidateAction, ContinuationStatus, ForecastRequest, StoredCommitment};

#[test]
fn forecast_scores_irreversible_actions_against_active_nonrevocable_commitments() {
    let release_continuation_id = "cont_release".to_string();
    let request = ForecastRequest {
        actions: vec![
            CandidateAction {
                name: "ship irreversible release".to_string(),
                continuation_id: Some(release_continuation_id.clone()),
                progress: 1.0,
                closure: 0.9,
                option_value_preserved: 0.6,
                risk: 0.2,
                irreversibility: 0.6,
                confusion: 0.1,
                friction: 0.1,
                temporal_debt_added: 0.1,
                uncertainty: 0.4,
                externality: 0.4,
            },
            CandidateAction {
                name: "verify rollback evidence".to_string(),
                continuation_id: Some(release_continuation_id.clone()),
                progress: 0.6,
                closure: 0.2,
                option_value_preserved: 0.8,
                risk: 0.05,
                irreversibility: 0.1,
                confusion: 0.1,
                friction: 0.2,
                temporal_debt_added: 0.0,
                uncertainty: 0.2,
                externality: 0.1,
            },
        ],
        relations: Vec::new(),
    };
    let commitments = vec![StoredCommitment {
        id: "commit_release".to_string(),
        continuation_id: release_continuation_id,
        speaker: "user".to_string(),
        statement: "Do not ship until rollback evidence is verified".to_string(),
        scope: Some("release".to_string()),
        deadline: None,
        revocable: false,
        status: ContinuationStatus::Active,
        created_at: "2026-05-07T00:00:00Z".to_string(),
    }];

    let without_commitment = ForecastScorer.score(&request);
    assert_eq!(
        without_commitment.ranked_actions[0].name,
        "ship irreversible release"
    );
    assert!(!without_commitment.ranked_actions[0].requires_confirmation);

    let with_commitment = ForecastScorer.score_with_commitments(&request, &commitments);
    assert_eq!(
        with_commitment.ranked_actions[0].name,
        "verify rollback evidence"
    );
    let risky_action = with_commitment
        .ranked_actions
        .iter()
        .find(|action| action.name == "ship irreversible release")
        .expect("risky action should be ranked");
    assert!(risky_action.requires_confirmation);
    assert!(risky_action.ask_before_act);
    assert!(risky_action
        .reason
        .contains("commitment_constraint_penalty"));
}

#[test]
fn forecast_ignores_inactive_or_unrelated_commitments() {
    let request = ForecastRequest {
        actions: vec![CandidateAction {
            name: "ship reversible draft".to_string(),
            continuation_id: Some("cont_draft".to_string()),
            progress: 0.8,
            closure: 0.4,
            option_value_preserved: 0.7,
            risk: 0.1,
            irreversibility: 0.2,
            confusion: 0.1,
            friction: 0.1,
            temporal_debt_added: 0.0,
            uncertainty: 0.2,
            externality: 0.2,
        }],
        relations: Vec::new(),
    };
    let commitments = vec![
        StoredCommitment {
            id: "commit_closed".to_string(),
            continuation_id: "cont_draft".to_string(),
            speaker: "user".to_string(),
            statement: "Closed commitment should not constrain scoring".to_string(),
            scope: None,
            deadline: None,
            revocable: false,
            status: ContinuationStatus::Closed,
            created_at: "2026-05-07T00:00:00Z".to_string(),
        },
        StoredCommitment {
            id: "commit_unrelated".to_string(),
            continuation_id: "cont_other".to_string(),
            speaker: "user".to_string(),
            statement: "Unrelated commitment should not constrain scoring".to_string(),
            scope: None,
            deadline: None,
            revocable: false,
            status: ContinuationStatus::Active,
            created_at: "2026-05-07T00:00:00Z".to_string(),
        },
    ];

    assert_eq!(
        ForecastScorer.score_with_commitments(&request, &commitments),
        ForecastScorer.score(&request)
    );
}
