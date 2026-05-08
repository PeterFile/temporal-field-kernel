use tfk_protocol::{
    CandidateAction, ContinuationDelta, ContinuationRelationEdge, ContinuationRelationKind,
    ContinuationStatus, ContinuationType, ForecastRequest, ForecastResult, LensActiveContinuation,
    LensBoundary, LensCard, LensPreferredAction, LensRequest, RankedAction, StoredCommitment,
    TemporalDebtWarning,
};
pub use tfk_protocol::{PreflightResult, PreflightSignals};
use tfk_rules::RuleEngine;
pub use tfk_rules::RuleFact;

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
        self.generate_with_relations_and_rule_facts(
            request,
            continuations,
            relations,
            &[],
            raw_event_count,
        )
    }

    pub fn generate_with_relations_and_rule_facts(
        &self,
        request: &LensRequest,
        continuations: &[TimeFieldContinuation],
        relations: &[ContinuationRelationEdge],
        rule_facts: &[RuleFact],
        raw_event_count: usize,
    ) -> LensCard {
        let mut active: Vec<_> = continuations
            .iter()
            .filter(|continuation| !is_closed(continuation.status))
            .filter_map(|continuation| active_continuation_for(continuation, request))
            .collect();
        apply_relation_kind_activation(&mut active, relations);
        apply_rule_fact_activation(&mut active, rule_facts);
        sort_active_continuations(&mut active);
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
                commitment_constraints: Vec::new(),
                advisory_forecast_signals: Vec::new(),
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
                commitment_constraints: Vec::new(),
                advisory_forecast_signals: Vec::new(),
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

pub fn lens_rule_facts(
    request: &LensRequest,
    continuations: &[TimeFieldContinuation],
) -> Vec<RuleFact> {
    let mut engine = RuleEngine::with_core_rules();
    let request_horizon_is_near = request_horizon_is_near(&request.horizon);

    for continuation in continuations
        .iter()
        .filter(|continuation| !is_closed(continuation.status))
    {
        engine.assert_fact_parts("continuation", [&continuation.id]);
        engine.assert_fact_parts(
            "continuation_status",
            [&continuation.id, status_fact_name(continuation.status)],
        );
        engine.assert_fact_parts(
            "continuation_type",
            [
                &continuation.id,
                continuation_type_fact_name(continuation.continuation_type),
            ],
        );

        let text = format!("{} {}", continuation.title, continuation.summary).to_lowercase();
        if contains_fact_marker(&text, &["risk_level"], "high") {
            engine.assert_fact_parts("risk_level", [&continuation.id, "high"]);
        }
        if request_horizon_is_near
            || contains_fact_marker(&text, &["time_horizon", "horizon"], "near")
        {
            engine.assert_fact_parts("time_horizon", [&continuation.id, "near"]);
        }
    }

    engine.evaluate();
    engine.facts().to_vec()
}

fn status_fact_name(status: ContinuationStatus) -> &'static str {
    match status {
        ContinuationStatus::Active => "active",
        ContinuationStatus::Stabilized => "stabilized",
        ContinuationStatus::Deferred => "deferred",
        ContinuationStatus::Closed => "closed",
        ContinuationStatus::Retired => "retired",
    }
}

fn continuation_type_fact_name(continuation_type: ContinuationType) -> &'static str {
    match continuation_type {
        ContinuationType::Obligation => "obligation",
        ContinuationType::Epistemic => "epistemic",
        ContinuationType::Relational => "relational",
        ContinuationType::Narrative => "narrative",
        ContinuationType::Risk => "risk",
        ContinuationType::Opportunity => "opportunity",
        ContinuationType::Rhythm => "rhythm",
    }
}

fn request_horizon_is_near(horizon: &[String]) -> bool {
    horizon.iter().any(|item| {
        matches!(
            item.trim().to_lowercase().as_str(),
            "near" | "now" | "next-action" | "next_action" | "next action"
        )
    })
}

fn contains_fact_marker(text: &str, keys: &[&str], value: &str) -> bool {
    keys.iter().any(|key| marker_has_value(text, key, value))
}

fn marker_has_value(text: &str, key: &str, value: &str) -> bool {
    let mut offset = 0;
    while let Some(relative_start) = text[offset..].find(key) {
        let start = offset + relative_start;
        let key_end = start + key.len();
        if start > 0 && is_marker_ident(text[..start].chars().next_back().unwrap()) {
            offset = key_end;
            continue;
        }

        let rest = text[key_end..].trim_start();
        let Some(rest) = rest.strip_prefix('=').or_else(|| rest.strip_prefix(':')) else {
            offset = key_end;
            continue;
        };
        let rest = rest.trim_start();
        if rest.starts_with(value)
            && rest[value.len()..]
                .chars()
                .next()
                .map_or(true, |ch| !is_marker_ident(ch))
        {
            return true;
        }
        offset = key_end;
    }

    false
}

fn is_marker_ident(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

fn apply_relation_kind_activation(
    active: &mut [LensActiveContinuation],
    relations: &[ContinuationRelationEdge],
) {
    for relation in relations {
        match relation.kind {
            ContinuationRelationKind::Supports => {
                scale_activation(active, &relation.to_id, 1.10);
            }
            ContinuationRelationKind::DependsOn => {
                scale_activation(active, &relation.to_id, 1.15);
                scale_activation(active, &relation.from_id, 0.95);
            }
            ContinuationRelationKind::Subsumes => {
                scale_activation(active, &relation.from_id, 1.15);
                scale_activation(active, &relation.to_id, 0.95);
            }
            ContinuationRelationKind::Conflicts | ContinuationRelationKind::Blocks => {}
        }
    }
}

fn scale_activation(active: &mut [LensActiveContinuation], id: &str, factor: f64) {
    if let Some(continuation) = active.iter_mut().find(|continuation| continuation.id == id) {
        continuation.activation *= factor;
    }
}

fn apply_rule_fact_activation(active: &mut [LensActiveContinuation], rule_facts: &[RuleFact]) {
    for fact in rule_facts {
        let Some(continuation_id) = fact.args.first() else {
            continue;
        };
        match fact.predicate.as_str() {
            "needs_review" => {
                scale_activation(active, continuation_id, 1.25);
                mark_verify_pressure(
                    active,
                    continuation_id,
                    "rules-derived review pressure requires verification",
                );
            }
            "risk_marker" if fact.args.get(1).is_some_and(|value| value == "review") => {
                scale_activation(active, continuation_id, 1.05);
            }
            "timing_attention" => {
                scale_activation(active, continuation_id, 1.10);
            }
            "path_choice" if fact.args.get(1).is_some_and(|value| value == "review_now") => {
                scale_activation(active, continuation_id, 1.20);
                mark_verify_pressure(
                    active,
                    continuation_id,
                    "rules-derived review_now path choice would be missed",
                );
            }
            _ => {}
        }
    }
}

fn mark_verify_pressure(active: &mut [LensActiveContinuation], id: &str, reason: &str) {
    if let Some(continuation) = active.iter_mut().find(|continuation| continuation.id == id) {
        continuation.recommended_delta = ContinuationDelta::Verify;
        continuation.risk_if_ignored = reason.to_string();
    }
}

fn sort_active_continuations(active: &mut [LensActiveContinuation]) {
    active.sort_by(|left, right| {
        right
            .activation
            .total_cmp(&left.activation)
            .then_with(|| left.id.cmp(&right.id))
            .then_with(|| left.title.cmp(&right.title))
    });
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
        commitment_constraints: Vec::new(),
        advisory_forecast_signals: Vec::new(),
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
        commitment_constraints: Vec::new(),
        advisory_forecast_signals: Vec::new(),
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
        commitment_constraints: Vec::new(),
        advisory_forecast_signals: Vec::new(),
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
        commitment_constraints: Vec::new(),
        advisory_forecast_signals: Vec::new(),
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
            commitment_constraints: Vec::new(),
            advisory_forecast_signals: Vec::new(),
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
            commitment_constraints: Vec::new(),
            advisory_forecast_signals: Vec::new(),
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
        ContinuationStatus::Closed | ContinuationStatus::Retired | ContinuationStatus::Stabilized
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
    if !allows_semantic_query_expansion(&query) {
        return 0.0;
    }

    let tokens = semantic_query_tokens(&query);
    if tokens.is_empty() {
        return 0.0;
    }
    let required_token_hits = tokens.len().min(2);
    let text_tokens = semantic_query_tokens(&text);
    let hits = tokens
        .iter()
        .filter(|token| text_tokens.iter().any(|text_token| text_token == *token))
        .count();
    if hits < required_token_hits {
        return 0.0;
    }

    0.5 + (0.4 * (hits as f64 / tokens.len() as f64))
}

fn allows_semantic_query_expansion(query: &str) -> bool {
    !query.chars().any(|ch| matches!(ch, '%' | '_' | '\\'))
}

fn semantic_query_tokens(query: &str) -> Vec<String> {
    let normalized: String = query
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect();
    let mut tokens = Vec::new();
    for token in normalized.split_whitespace() {
        if !is_semantic_query_token(token) || tokens.iter().any(|existing| existing == token) {
            continue;
        }
        tokens.push(token.to_string());
    }
    tokens
}

fn is_semantic_query_token(token: &str) -> bool {
    token.chars().count() >= 3
        && !matches!(
            token,
            "and"
                | "are"
                | "but"
                | "for"
                | "from"
                | "not"
                | "the"
                | "that"
                | "this"
                | "with"
                | "into"
                | "onto"
        )
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
        self.score_with_commitments(request, &[])
    }

    pub fn score_with_commitments(
        &self,
        request: &ForecastRequest,
        commitments: &[StoredCommitment],
    ) -> ForecastResult {
        let mut ranked_actions: Vec<_> = request
            .actions
            .iter()
            .map(|action| {
                let matching_commitments = active_commitments_for_action(action, commitments);
                let commitment_penalty = commitment_constraint_penalty(action, &matching_commitments);
                let mut score = clamp01(action.progress)
                    + clamp01(action.closure)
                    + clamp01(action.option_value_preserved)
                    - clamp01(action.risk)
                    - clamp01(action.irreversibility)
                    - clamp01(action.confusion)
                    - clamp01(action.friction)
                    - clamp01(action.temporal_debt_added)
                    - commitment_penalty;
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
                        > 0.5
                        || commitment_requires_confirmation(action, &matching_commitments);
                let ask_score = clamp01(action.uncertainty) + clamp01(action.risk)
                    - clamp01(action.friction)
                    - clamp01(action.progress);
                let ask_before_act = requires_confirmation || ask_score > score;
                let reason = if matching_commitments.is_empty() {
                    "progress + closure + option_value_preserved - risk - irreversibility - confusion - friction - temporal_debt_added".to_string()
                } else {
                    "progress + closure + option_value_preserved - risk - irreversibility - confusion - friction - temporal_debt_added - commitment_constraint_penalty".to_string()
                };

                RankedAction {
                    name: action.name.clone(),
                    score,
                    requires_confirmation,
                    ask_before_act,
                    reason,
                }
            })
            .collect();
        ranked_actions.sort_by(|left, right| right.score.total_cmp(&left.score));
        ForecastResult {
            ranked_actions,
            advisory_signals: Vec::new(),
        }
    }
}

fn active_commitments_for_action<'a>(
    action: &CandidateAction,
    commitments: &'a [StoredCommitment],
) -> Vec<&'a StoredCommitment> {
    let Some(continuation_id) = action.continuation_id.as_deref() else {
        return Vec::new();
    };
    commitments
        .iter()
        .filter(|commitment| commitment.status == ContinuationStatus::Active)
        .filter(|commitment| commitment.continuation_id == continuation_id)
        .collect()
}

fn commitment_constraint_penalty(
    action: &CandidateAction,
    commitments: &[&StoredCommitment],
) -> f64 {
    if commitments.is_empty() {
        return 0.0;
    }
    let commitment_weight = commitments
        .iter()
        .map(|commitment| if commitment.revocable { 0.25 } else { 0.55 })
        .fold(0.0, f64::max);
    commitment_weight * commitment_consequence(action)
}

fn commitment_requires_confirmation(
    action: &CandidateAction,
    commitments: &[&StoredCommitment],
) -> bool {
    commitments
        .iter()
        .any(|commitment| !commitment.revocable && commitment_consequence(action) >= 0.5)
}

fn commitment_consequence(action: &CandidateAction) -> f64 {
    clamp01(action.irreversibility)
        .max(clamp01(action.risk))
        .max(clamp01(action.temporal_debt_added))
}

fn is_constrained_by_explicit_relation(id: &str, relations: &[ContinuationRelationEdge]) -> bool {
    relations.iter().any(|relation| match relation.kind {
        ContinuationRelationKind::Conflicts => relation.from_id == id || relation.to_id == id,
        ContinuationRelationKind::Blocks => relation.to_id == id,
        _ => false,
    })
}
