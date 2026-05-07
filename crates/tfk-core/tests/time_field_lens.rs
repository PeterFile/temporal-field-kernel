use tfk_core::{ForecastScorer, TimeFieldContinuation, TimeFieldLensEngine};
use tfk_protocol::{
    CandidateAction, ContinuationDelta, ContinuationRelationEdge, ContinuationRelationKind,
    ContinuationStatus, ContinuationType, ForecastRequest, LensRequest,
};

#[test]
fn lens_turns_matching_obligation_into_future_path_constraint() {
    let request = lens_request("项目状态机");
    let candidates = vec![TimeFieldContinuation {
        id: "cont_obligation".to_string(),
        title: "项目状态机不是目标".to_string(),
        summary: "后续方案不能以 task state / project memory 为主轴".to_string(),
        continuation_type: ContinuationType::Obligation,
        status: ContinuationStatus::Active,
    }];

    let card = TimeFieldLensEngine.generate(&request, &candidates, 0);

    assert_eq!(card.stance, "act");
    assert_eq!(card.active_continuations.len(), 1);
    let active = &card.active_continuations[0];
    assert_eq!(active.id, "cont_obligation");
    assert_eq!(active.continuation_type, ContinuationType::Obligation);
    assert!(active.pressure > 0.8, "pressure was {}", active.pressure);
    assert_eq!(active.recommended_delta, ContinuationDelta::Advance);
    assert!(active.risk_if_ignored.contains("commitment"));
    assert!(card
        .avoid
        .iter()
        .any(|item| item.contains("ignore active obligation")));
    let preferred = card.preferred_action.as_ref().expect("preferred action");
    assert!(preferred.name.contains("advance"));
    assert!(preferred.score > 0.0);
}

#[test]
fn lens_surfaces_conflict_between_progress_and_risk_instead_of_collapsing_it() {
    let request = lens_request("release");
    let candidates = vec![
        TimeFieldContinuation {
            id: "cont_ship".to_string(),
            title: "release quickly".to_string(),
            summary: "ship the public interface".to_string(),
            continuation_type: ContinuationType::Obligation,
            status: ContinuationStatus::Active,
        },
        TimeFieldContinuation {
            id: "cont_breakage".to_string(),
            title: "release breakage risk".to_string(),
            summary: "public API could break existing users".to_string(),
            continuation_type: ContinuationType::Risk,
            status: ContinuationStatus::Active,
        },
    ];

    let card = TimeFieldLensEngine.generate(&request, &candidates, 0);

    assert_eq!(card.stance, "verify");
    assert!(card.boundaries.iter().any(|boundary| {
        boundary.kind == "temporal_conflict" && boundary.status == "needs_resolution"
    }));
    assert!(card.why_now.contains("conflict"));
    assert!(card
        .open_questions
        .iter()
        .any(|question| question.contains("resolve")));
}

#[test]
fn explicit_conflict_and_block_relations_emit_boundaries() {
    let request = lens_request("release");
    let candidates = vec![
        TimeFieldContinuation {
            id: "cont_ship".to_string(),
            title: "release quickly".to_string(),
            summary: "ship the public interface".to_string(),
            continuation_type: ContinuationType::Obligation,
            status: ContinuationStatus::Active,
        },
        TimeFieldContinuation {
            id: "cont_verify".to_string(),
            title: "release verification".to_string(),
            summary: "verify compatibility before public release".to_string(),
            continuation_type: ContinuationType::Risk,
            status: ContinuationStatus::Active,
        },
        TimeFieldContinuation {
            id: "cont_docs".to_string(),
            title: "release docs".to_string(),
            summary: "publish release notes".to_string(),
            continuation_type: ContinuationType::Obligation,
            status: ContinuationStatus::Active,
        },
    ];
    let relations = vec![
        ContinuationRelationEdge {
            from_id: "cont_ship".to_string(),
            to_id: "cont_verify".to_string(),
            kind: ContinuationRelationKind::Conflicts,
            reason: Some("speed conflicts with verification".to_string()),
        },
        ContinuationRelationEdge {
            from_id: "cont_verify".to_string(),
            to_id: "cont_docs".to_string(),
            kind: ContinuationRelationKind::Blocks,
            reason: Some("docs should wait for verification".to_string()),
        },
    ];

    let card = TimeFieldLensEngine.generate_with_relations(&request, &candidates, &relations, 0);

    assert!(card.boundaries.iter().any(|boundary| {
        boundary.kind == "relation_conflict" && boundary.status == "needs_resolution"
    }));
    assert!(card
        .boundaries
        .iter()
        .any(|boundary| boundary.kind == "relation_block" && boundary.status == "blocked"));
    assert!(card.preferred_action.is_none());
}

#[test]
fn blocked_matching_continuation_is_constrained_even_when_blocker_does_not_match_query() {
    let request = lens_request("release docs");
    let candidates = vec![
        TimeFieldContinuation {
            id: "cont_verify".to_string(),
            title: "compatibility verification".to_string(),
            summary: "finish API compatibility checks".to_string(),
            continuation_type: ContinuationType::Risk,
            status: ContinuationStatus::Active,
        },
        TimeFieldContinuation {
            id: "cont_docs".to_string(),
            title: "release docs".to_string(),
            summary: "publish release notes".to_string(),
            continuation_type: ContinuationType::Obligation,
            status: ContinuationStatus::Active,
        },
    ];
    let relations = vec![ContinuationRelationEdge {
        from_id: "cont_verify".to_string(),
        to_id: "cont_docs".to_string(),
        kind: ContinuationRelationKind::Blocks,
        reason: Some("docs should wait for verification".to_string()),
    }];

    let card = TimeFieldLensEngine.generate_with_relations(&request, &candidates, &relations, 0);

    assert_eq!(card.stance, "verify");
    assert!(card
        .boundaries
        .iter()
        .any(|boundary| boundary.kind == "relation_block" && boundary.status == "blocked"));
    assert!(card.preferred_action.is_none());
}

#[test]
fn supports_relation_boosts_supported_target_in_lens_ranking() {
    let request = lens_request("release");
    let candidates = vec![
        active_opportunity("a_supporter", "release supporter"),
        active_opportunity("z_target", "release target"),
    ];
    let relations = vec![ContinuationRelationEdge {
        from_id: "a_supporter".to_string(),
        to_id: "z_target".to_string(),
        kind: ContinuationRelationKind::Supports,
        reason: Some("supporter unlocks the target path".to_string()),
    }];

    let card = TimeFieldLensEngine.generate_with_relations(&request, &candidates, &relations, 0);

    assert_eq!(card.active_continuations[0].id, "z_target");
    assert!(card.active_continuations[0].activation > card.active_continuations[1].activation);
}

#[test]
fn depends_on_relation_boosts_prerequisite_in_lens_ranking() {
    let request = lens_request("release");
    let candidates = vec![
        active_opportunity("a_dependent", "release dependent"),
        active_opportunity("z_prerequisite", "release prerequisite"),
    ];
    let relations = vec![ContinuationRelationEdge {
        from_id: "a_dependent".to_string(),
        to_id: "z_prerequisite".to_string(),
        kind: ContinuationRelationKind::DependsOn,
        reason: Some("dependent should wait on prerequisite".to_string()),
    }];

    let card = TimeFieldLensEngine.generate_with_relations(&request, &candidates, &relations, 0);

    assert_eq!(card.active_continuations[0].id, "z_prerequisite");
    assert!(card.active_continuations[0].activation > card.active_continuations[1].activation);
}

#[test]
fn subsumes_relation_boosts_parent_in_lens_ranking() {
    let request = lens_request("release");
    let candidates = vec![
        active_opportunity("a_child", "release child"),
        active_opportunity("z_parent", "release parent"),
    ];
    let relations = vec![ContinuationRelationEdge {
        from_id: "z_parent".to_string(),
        to_id: "a_child".to_string(),
        kind: ContinuationRelationKind::Subsumes,
        reason: Some("parent captures the child path".to_string()),
    }];

    let card = TimeFieldLensEngine.generate_with_relations(&request, &candidates, &relations, 0);

    assert_eq!(card.active_continuations[0].id, "z_parent");
    assert!(card.active_continuations[0].activation > card.active_continuations[1].activation);
}

#[test]
fn lens_ranking_uses_deterministic_tie_break_when_activation_matches() {
    let request = lens_request("release");
    let candidates = vec![
        active_opportunity("c_tie", "release c"),
        active_opportunity("a_tie", "release a"),
        active_opportunity("b_tie", "release b"),
    ];

    let card = TimeFieldLensEngine.generate(&request, &candidates, 0);

    let ids: Vec<_> = card
        .active_continuations
        .iter()
        .map(|continuation| continuation.id.as_str())
        .collect();
    assert_eq!(ids, vec!["a_tie", "b_tie", "c_tie"]);
}

#[test]
fn forecast_penalizes_blocked_target_not_blocking_prerequisite() {
    let request = ForecastRequest {
        actions: vec![
            CandidateAction {
                name: "finish verification".to_string(),
                continuation_id: Some("cont_verify".to_string()),
                progress: 0.6,
                closure: 0.4,
                option_value_preserved: 0.4,
                risk: 0.1,
                irreversibility: 0.1,
                confusion: 0.1,
                friction: 0.1,
                temporal_debt_added: 0.0,
                uncertainty: 0.1,
                externality: 0.1,
            },
            CandidateAction {
                name: "publish docs before verification".to_string(),
                continuation_id: Some("cont_docs".to_string()),
                progress: 0.6,
                closure: 0.4,
                option_value_preserved: 0.4,
                risk: 0.1,
                irreversibility: 0.1,
                confusion: 0.1,
                friction: 0.1,
                temporal_debt_added: 0.0,
                uncertainty: 0.1,
                externality: 0.1,
            },
        ],
        relations: vec![ContinuationRelationEdge {
            from_id: "cont_verify".to_string(),
            to_id: "cont_docs".to_string(),
            kind: ContinuationRelationKind::Blocks,
            reason: Some("docs should wait for verification".to_string()),
        }],
    };

    let result = ForecastScorer.score(&request);

    assert_eq!(result.ranked_actions[0].name, "finish verification");
    assert!(result.ranked_actions[0].score > result.ranked_actions[1].score);
}

#[test]
fn forecast_ranks_high_progress_low_risk_action_first() {
    let request = ForecastRequest {
        actions: vec![
            CandidateAction {
                name: "ship unverified API".to_string(),
                continuation_id: Some("cont_ship".to_string()),
                progress: 0.9,
                closure: 0.3,
                option_value_preserved: 0.1,
                risk: 0.8,
                irreversibility: 0.8,
                confusion: 0.4,
                friction: 0.3,
                temporal_debt_added: 0.6,
                uncertainty: 0.9,
                externality: 0.8,
            },
            CandidateAction {
                name: "verify then draft release".to_string(),
                continuation_id: Some("cont_verify".to_string()),
                progress: 0.7,
                closure: 0.6,
                option_value_preserved: 0.8,
                risk: 0.1,
                irreversibility: 0.1,
                confusion: 0.1,
                friction: 0.2,
                temporal_debt_added: 0.0,
                uncertainty: 0.2,
                externality: 0.3,
            },
        ],
        relations: Vec::new(),
    };

    let result = ForecastScorer.score(&request);

    assert_eq!(result.ranked_actions[0].name, "verify then draft release");
    assert!(result.ranked_actions[0].score > result.ranked_actions[1].score);
    assert!(result.ranked_actions[1].requires_confirmation);
    assert!(result.ranked_actions[1].ask_before_act);
}

#[test]
fn inactive_continuations_do_not_create_pressure_or_fake_action() {
    let request = lens_request("项目状态机");
    let candidates: Vec<_> = [
        ContinuationStatus::Closed,
        ContinuationStatus::Retired,
        ContinuationStatus::Stabilized,
    ]
    .into_iter()
    .map(|status| TimeFieldContinuation {
        id: format!("cont_{status:?}"),
        title: "项目状态机不是目标".to_string(),
        summary: "already resolved".to_string(),
        continuation_type: ContinuationType::Obligation,
        status,
    })
    .collect();

    let card = TimeFieldLensEngine.generate(&request, &candidates, 0);

    assert_eq!(card.stance, "wait");
    assert!(card.active_continuations.is_empty());
    assert!(card.preferred_action.is_none());
}

#[test]
fn unresolved_risk_and_questions_emit_temporal_debt_and_verify_action() {
    let request = lens_request("验证");
    let candidates = vec![
        TimeFieldContinuation {
            id: "cont_hypothesis".to_string(),
            title: "验证接口假设".to_string(),
            summary: "未验证结论会污染下一步动作".to_string(),
            continuation_type: ContinuationType::Epistemic,
            status: ContinuationStatus::Active,
        },
        TimeFieldContinuation {
            id: "cont_risk".to_string(),
            title: "验证发布风险".to_string(),
            summary: "公共接口可能破坏用户空间".to_string(),
            continuation_type: ContinuationType::Risk,
            status: ContinuationStatus::Active,
        },
    ];

    let card = TimeFieldLensEngine.generate(&request, &candidates, 0);

    assert_eq!(card.stance, "verify");
    assert!(card
        .temporal_debt
        .as_ref()
        .is_some_and(|debt| { debt.score > 0.6 && debt.reason.contains("unresolved") }));
    let preferred = card.preferred_action.as_ref().expect("preferred action");
    assert!(preferred.name.contains("verify"));
}

fn active_opportunity(id: &str, title: &str) -> TimeFieldContinuation {
    TimeFieldContinuation {
        id: id.to_string(),
        title: title.to_string(),
        summary: "release ranking candidate".to_string(),
        continuation_type: ContinuationType::Opportunity,
        status: ContinuationStatus::Active,
    }
}

fn lens_request(query: &str) -> LensRequest {
    LensRequest {
        query: query.to_string(),
        horizon: Vec::new(),
        perspective: Vec::new(),
    }
}
