use serde::{Deserialize, Serialize};
use tfk_protocol::{
    ContinuationDelta, ContinuationRelationEdge, ContinuationRelationKind, ContinuationStatus,
    ContinuationType, ForecastRequest, ForecastResult, LensActiveContinuation, LensBoundary,
    LensCard, LensPreferredAction, LensRequest, RankedAction, TemporalDebtWarning,
};

#[derive(Debug, Clone, PartialEq)]
pub struct TimeFieldContinuation {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub continuation_type: ContinuationType,
    pub status: ContinuationStatus,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TimeFieldLensEngine;

impl TimeFieldLensEngine {
    pub fn generate(
        &self,
        request: &LensRequest,
        continuations: &[TimeFieldContinuation],
        raw_event_count: usize,
    ) -> LensCard {
        self.generate_with_relations(request, continuations, &[], raw_event_count)
    }

    pub fn generate_with_relations(
        &self,
        request: &LensRequest,
        continuations: &[TimeFieldContinuation],
        relations: &[ContinuationRelationEdge],
        raw_event_count: usize,
    ) -> LensCard {
        let mut active: Vec<_> = continuations
            .iter()
            .filter(|continuation| !is_closed(continuation.status))
            .filter_map(|continuation| active_continuation_for(continuation, request))
            .collect();
        active.sort_by(|left, right| right.activation.total_cmp(&left.activation));
        let temporal_debt = temporal_debt(&active);

        if active.is_empty() {
            return raw_or_empty_card(request, raw_event_count);
        }

        let relation_boundaries = relation_boundaries(&active, relations);
        if !relation_boundaries.is_empty() {
            return LensCard {
                stance: "verify".to_string(),
                why_now: format!(
                    "explicit continuation relations constrain query: {}",
                    request.query
                ),
                active_continuations: active,
                boundaries: relation_boundaries,
                avoid: vec![
                    "do not collapse related continuations into one goal".to_string(),
                    "do not advance a blocked continuation".to_string(),
                ],
                preferred_action: None,
                open_questions: vec![
                    "resolve conflicting or blocked continuations before action".to_string()
                ],
                temporal_debt,
            };
        }

        if has_progress_risk_conflict(&active) {
            return LensCard {
                stance: "verify".to_string(),
                why_now: format!(
                    "matching continuations create a conflict for query: {}",
                    request.query
                ),
                active_continuations: active,
                boundaries: vec![LensBoundary {
                    kind: "temporal_conflict".to_string(),
                    status: "needs_resolution".to_string(),
                    reason: Some("progress pressure conflicts with breakage risk".to_string()),
                }],
                avoid: vec![
                    "do not collapse progress and risk into a single recall".to_string(),
                    "do not advance without resolving the temporal conflict".to_string(),
                ],
                preferred_action: None,
                open_questions: vec![
                    "resolve which constraint dominates before taking action".to_string()
                ],
                temporal_debt,
            };
        }

        match active[0].continuation_type {
            ContinuationType::Risk | ContinuationType::Epistemic => {
                verification_card(request, active, temporal_debt)
            }
            ContinuationType::Narrative | ContinuationType::Rhythm => {
                continuation_recall_card(request, active, temporal_debt)
            }
            ContinuationType::Relational => repair_card(request, active, temporal_debt),
            ContinuationType::Obligation | ContinuationType::Opportunity => {
                action_card(request, active, temporal_debt)
            }
        }
    }
}

fn relation_boundaries(
    active: &[LensActiveContinuation],
    relations: &[ContinuationRelationEdge],
) -> Vec<LensBoundary> {
    relations
        .iter()
        .filter_map(|relation| match relation.kind {
            ContinuationRelationKind::Conflicts
                if active
                    .iter()
                    .any(|item| item.id == relation.from_id || item.id == relation.to_id) =>
            {
                Some(LensBoundary {
                    kind: "relation_conflict".to_string(),
                    status: "needs_resolution".to_string(),
                    reason: relation.reason.clone(),
                })
            }
            ContinuationRelationKind::Blocks
                if active.iter().any(|item| item.id == relation.to_id) =>
            {
                Some(LensBoundary {
                    kind: "relation_block".to_string(),
                    status: "blocked".to_string(),
                    reason: relation.reason.clone(),
                })
            }
            _ => None,
        })
        .collect()
}

fn active_continuation_for(
    continuation: &TimeFieldContinuation,
    request: &LensRequest,
) -> Option<LensActiveContinuation> {
    let semantic_match = semantic_match(continuation, &request.query);
    if semantic_match <= 0.0 {
        return None;
    }
    let pressure = pressure(continuation);
    let activation = semantic_match
        * horizon_overlap(continuation, &request.horizon)
        * perspective_weight(continuation, &request.perspective)
        * pressure
        * confidence(continuation);
    if activation <= 0.0 {
        return None;
    }

    Some(LensActiveContinuation {
        id: continuation.id.clone(),
        title: continuation.title.clone(),
        continuation_type: continuation.continuation_type,
        pressure,
        activation,
        risk_if_ignored: risk_if_ignored(continuation.continuation_type).to_string(),
        recommended_delta: recommended_delta(continuation.continuation_type),
    })
}

fn action_card(
    request: &LensRequest,
    active: Vec<LensActiveContinuation>,
    temporal_debt: Option<TemporalDebtWarning>,
) -> LensCard {
    let primary = &active[0];
    LensCard {
        stance: "act".to_string(),
        why_now: format!(
            "active continuation constrains the next action for query: {}",
            request.query
        ),
        boundaries: Vec::new(),
        avoid: vec![
            "do not ignore active obligation pressure".to_string(),
            "do not turn the lens into memoir or raw recall".to_string(),
        ],
        preferred_action: Some(LensPreferredAction {
            name: format!("advance {}", primary.id),
            reason: "max continuation progress with low irreversibility".to_string(),
            score: primary.activation,
        }),
        open_questions: vec!["none_blocking".to_string()],
        temporal_debt,
        active_continuations: active,
    }
}

fn verification_card(
    request: &LensRequest,
    active: Vec<LensActiveContinuation>,
    temporal_debt: Option<TemporalDebtWarning>,
) -> LensCard {
    let primary = &active[0];
    LensCard {
        stance: "verify".to_string(),
        why_now: format!(
            "risk or unresolved question changes the next action for query: {}",
            request.query
        ),
        boundaries: vec![LensBoundary {
            kind: "evidence_boundary".to_string(),
            status: "needs_verification".to_string(),
            reason: Some(primary.risk_if_ignored.clone()),
        }],
        avoid: vec![
            "do not act as if unresolved risk is closed".to_string(),
            "do not promote unverified claims into commitments".to_string(),
        ],
        preferred_action: Some(LensPreferredAction {
            name: format!("verify {}", primary.id),
            reason: "value of information beats premature action".to_string(),
            score: primary.activation,
        }),
        open_questions: vec!["what evidence would change the path choice?".to_string()],
        temporal_debt,
        active_continuations: active,
    }
}

fn repair_card(
    request: &LensRequest,
    active: Vec<LensActiveContinuation>,
    temporal_debt: Option<TemporalDebtWarning>,
) -> LensCard {
    let primary = &active[0];
    LensCard {
        stance: "repair".to_string(),
        why_now: format!(
            "relational continuation constrains the next action for query: {}",
            request.query
        ),
        boundaries: Vec::new(),
        avoid: vec!["do not optimize progress while relationship damage is unresolved".to_string()],
        preferred_action: Some(LensPreferredAction {
            name: format!("repair {}", primary.id),
            reason: "repair policy preserves future agency and trust".to_string(),
            score: primary.activation,
        }),
        open_questions: vec!["what acknowledgement or correction repairs this path?".to_string()],
        temporal_debt,
        active_continuations: active,
    }
}

fn continuation_recall_card(
    request: &LensRequest,
    active: Vec<LensActiveContinuation>,
    temporal_debt: Option<TemporalDebtWarning>,
) -> LensCard {
    LensCard {
        stance: "continuation_recall".to_string(),
        why_now: format!(
            "{} matching continuation(s) found for query: {}",
            active.len(),
            request.query
        ),
        boundaries: Vec::new(),
        avoid: vec![
            "do not infer closure from recall alone".to_string(),
            "do not expand this into vector search or full Datalog".to_string(),
        ],
        preferred_action: Some(LensPreferredAction {
            name: format!("stabilize {}", active[0].id),
            reason: "keep long-lived continuity explicit without forcing closure".to_string(),
            score: active[0].activation,
        }),
        open_questions: vec!["what is the next concrete observation or action?".to_string()],
        temporal_debt,
        active_continuations: active,
    }
}

fn raw_or_empty_card(request: &LensRequest, raw_event_count: usize) -> LensCard {
    if raw_event_count == 0 {
        LensCard {
            stance: "wait".to_string(),
            why_now: format!(
                "no active matching continuations found for query: {}",
                request.query
            ),
            active_continuations: Vec::new(),
            boundaries: Vec::new(),
            avoid: vec!["do not invent action pressure without an active continuation".to_string()],
            preferred_action: None,
            open_questions: vec!["what evidence should be observed next?".to_string()],
            temporal_debt: None,
        }
    } else {
        LensCard {
            stance: "grounded_recall".to_string(),
            why_now: format!(
                "{raw_event_count} matching raw event(s) found for query: {}",
                request.query
            ),
            active_continuations: Vec::new(),
            boundaries: Vec::new(),
            avoid: vec![
                "do not infer closure from raw recall alone".to_string(),
                "do not treat this as a full continuation graph yet".to_string(),
            ],
            preferred_action: None,
            open_questions: vec![
                "which recalled event should become an active continuation?".to_string()
            ],
            temporal_debt: None,
        }
    }
}

fn is_closed(status: ContinuationStatus) -> bool {
    matches!(
        status,
        ContinuationStatus::Closed | ContinuationStatus::Retired
    )
}

fn semantic_match(continuation: &TimeFieldContinuation, query: &str) -> f64 {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return 0.0;
    }

    let text = format!("{} {}", continuation.title, continuation.summary).to_lowercase();
    if text.contains(&query) {
        return 1.0;
    }

    let mut tokens = query.split_whitespace().filter(|token| !token.is_empty());
    if tokens.any(|token| text.contains(token)) {
        0.7
    } else {
        0.0
    }
}

fn horizon_overlap(_continuation: &TimeFieldContinuation, horizon: &[String]) -> f64 {
    if horizon.is_empty() {
        1.0
    } else {
        0.9
    }
}

fn perspective_weight(continuation: &TimeFieldContinuation, perspective: &[String]) -> f64 {
    if perspective.is_empty() {
        return 1.0;
    }

    let joined = perspective.join(" ").to_lowercase();
    match continuation.continuation_type {
        ContinuationType::Risk if contains_any(&joined, &["safety", "risk", "boundary"]) => 1.15,
        ContinuationType::Obligation
            if contains_any(&joined, &["planning", "commitment", "action"]) =>
        {
            1.10
        }
        ContinuationType::Opportunity
            if contains_any(&joined, &["planning", "opportunity", "action"]) =>
        {
            1.10
        }
        ContinuationType::Epistemic
            if contains_any(&joined, &["research", "verify", "evidence"]) =>
        {
            1.10
        }
        ContinuationType::Relational
            if contains_any(&joined, &["relationship", "trust", "repair"]) =>
        {
            1.10
        }
        ContinuationType::Narrative
            if contains_any(&joined, &["creative", "narrative", "identity"]) =>
        {
            1.10
        }
        ContinuationType::Rhythm if contains_any(&joined, &["rhythm", "cadence", "habit"]) => 1.10,
        _ => 0.95,
    }
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn temporal_debt(active: &[LensActiveContinuation]) -> Option<TemporalDebtWarning> {
    let score = active
        .iter()
        .map(|continuation| {
            let weight = match continuation.continuation_type {
                ContinuationType::Risk => 0.45,
                ContinuationType::Epistemic => 0.35,
                ContinuationType::Obligation => 0.25,
                ContinuationType::Relational => 0.20,
                ContinuationType::Opportunity => 0.10,
                ContinuationType::Rhythm => 0.10,
                ContinuationType::Narrative => 0.05,
            };
            continuation.pressure * weight
        })
        .sum::<f64>()
        .clamp(0.0, 1.0);

    if score < 0.5 {
        return None;
    }

    let level = if score >= 0.75 { "high" } else { "medium" };
    Some(TemporalDebtWarning {
        score,
        level: level.to_string(),
        reason: "unresolved risk, question, or obligation pressure is accumulating".to_string(),
    })
}

fn pressure(continuation: &TimeFieldContinuation) -> f64 {
    let base = match continuation.continuation_type {
        ContinuationType::Obligation => 0.95,
        ContinuationType::Risk => 0.90,
        ContinuationType::Opportunity => 0.75,
        ContinuationType::Epistemic => 0.65,
        ContinuationType::Relational => 0.60,
        ContinuationType::Rhythm => 0.55,
        ContinuationType::Narrative => 0.40,
    };
    let status_factor = match continuation.status {
        ContinuationStatus::Active => 1.0,
        ContinuationStatus::Stabilized => 0.6,
        ContinuationStatus::Deferred => 0.35,
        ContinuationStatus::Closed | ContinuationStatus::Retired => 0.0,
    };
    base * status_factor
}

fn confidence(continuation: &TimeFieldContinuation) -> f64 {
    if continuation.summary.trim().is_empty() {
        0.8
    } else {
        0.95
    }
}

fn recommended_delta(continuation_type: ContinuationType) -> ContinuationDelta {
    match continuation_type {
        ContinuationType::Risk | ContinuationType::Epistemic => ContinuationDelta::Verify,
        ContinuationType::Narrative => ContinuationDelta::Defer,
        _ => ContinuationDelta::Advance,
    }
}

fn risk_if_ignored(continuation_type: ContinuationType) -> &'static str {
    match continuation_type {
        ContinuationType::Obligation => "commitment violation or user correction",
        ContinuationType::Risk => "breakage risk becomes hidden temporal debt",
        ContinuationType::Opportunity => "available path loses option value",
        ContinuationType::Epistemic => "uncertainty contaminates the next action",
        ContinuationType::Relational => "relationship constraint is missed",
        ContinuationType::Rhythm => "cadence drifts without explicit choice",
        ContinuationType::Narrative => "context recall remains unresolved",
    }
}

fn has_progress_risk_conflict(active: &[LensActiveContinuation]) -> bool {
    let has_progress = active.iter().any(|continuation| {
        matches!(
            continuation.continuation_type,
            ContinuationType::Obligation | ContinuationType::Opportunity
        )
    });
    let has_risk = active
        .iter()
        .any(|continuation| continuation.continuation_type == ContinuationType::Risk);
    has_progress && has_risk
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PreflightSignals {
    pub uncertainty: f64,
    pub irreversibility: f64,
    pub externality: f64,
    pub option_value_loss: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreflightResult {
    pub requires_confirmation: bool,
    pub risk_product: f64,
    pub threshold: f64,
    pub reason: String,
    pub safer_alternative: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreflightScorer {
    threshold: f64,
}

impl PreflightScorer {
    pub fn with_threshold(threshold: f64) -> Self {
        Self { threshold }
    }

    pub fn score(&self, signals: PreflightSignals) -> PreflightResult {
        let uncertainty = clamp01(signals.uncertainty);
        let irreversibility = clamp01(signals.irreversibility);
        let externality = clamp01(signals.externality);
        let risk_product = uncertainty * irreversibility * externality;
        let requires_confirmation = risk_product > self.threshold;
        let reason = if requires_confirmation {
            "uncertainty * irreversibility * externality exceeds threshold"
        } else {
            "uncertainty * irreversibility * externality is below threshold"
        }
        .to_string();
        let safer_alternative = if requires_confirmation {
            Some("ask for confirmation or produce a reversible draft/dry-run".to_string())
        } else {
            None
        };

        PreflightResult {
            requires_confirmation,
            risk_product,
            threshold: self.threshold,
            reason,
            safer_alternative,
        }
    }
}

fn clamp01(value: f64) -> f64 {
    if value.is_nan() {
        return 0.0;
    }
    value.clamp(0.0, 1.0)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ForecastScorer;

impl ForecastScorer {
    pub fn score(&self, request: &ForecastRequest) -> ForecastResult {
        let mut ranked_actions: Vec<_> = request
            .actions
            .iter()
            .map(|action| {
                let mut score = clamp01(action.progress)
                    + clamp01(action.closure)
                    + clamp01(action.option_value_preserved)
                    - clamp01(action.risk)
                    - clamp01(action.irreversibility)
                    - clamp01(action.confusion)
                    - clamp01(action.friction)
                    - clamp01(action.temporal_debt_added);
                if action
                    .continuation_id
                    .as_ref()
                    .is_some_and(|id| is_constrained_by_explicit_relation(id, &request.relations))
                {
                    score -= 0.5;
                }

                let requires_confirmation =
                    clamp01(action.uncertainty) * clamp01(action.irreversibility)
                        * clamp01(action.externality)
                        > 0.5;
                let ask_score = clamp01(action.uncertainty) + clamp01(action.risk)
                    - clamp01(action.friction)
                    - clamp01(action.progress);
                let ask_before_act = requires_confirmation || ask_score > score;

                RankedAction {
                    name: action.name.clone(),
                    score,
                    requires_confirmation,
                    ask_before_act,
                    reason: "progress + closure + option_value_preserved - risk - irreversibility - confusion - friction - temporal_debt_added".to_string(),
                }
            })
            .collect();
        ranked_actions.sort_by(|left, right| right.score.total_cmp(&left.score));
        ForecastResult { ranked_actions }
    }
}

fn is_constrained_by_explicit_relation(id: &str, relations: &[ContinuationRelationEdge]) -> bool {
    relations.iter().any(|relation| match relation.kind {
        ContinuationRelationKind::Conflicts => relation.from_id == id || relation.to_id == id,
        ContinuationRelationKind::Blocks => relation.to_id == id,
        _ => false,
    })
}
