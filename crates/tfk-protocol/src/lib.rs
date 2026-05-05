use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    User,
    Agent,
    Tool,
    World,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventModality {
    Text,
    Action,
    ToolResult,
    Environment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    Observed,
    UserAsserted,
    Inferred,
    Predicted,
    Normative,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RawEventInput {
    pub session_id: String,
    pub adapter_id: String,
    pub source: EventSource,
    pub modality: EventModality,
    pub content: String,
    pub act_type: Option<String>,
    pub evidence_status: EvidenceStatus,
    pub time_utc: Option<String>,
}

impl RawEventInput {
    pub fn new_text(
        session_id: impl Into<String>,
        adapter_id: impl Into<String>,
        source: EventSource,
        content: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            adapter_id: adapter_id.into(),
            source,
            modality: EventModality::Text,
            content: content.into(),
            act_type: None,
            evidence_status: EvidenceStatus::Observed,
            time_utc: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProvenanceRef {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ApiEnvelope<T> {
    pub request_id: String,
    pub trace_id: String,
    pub ok: bool,
    pub data: Option<T>,
    pub warnings: Vec<String>,
    pub provenance: Vec<ProvenanceRef>,
}

impl<T> ApiEnvelope<T> {
    pub fn ok(request_id: impl Into<String>, trace_id: impl Into<String>, data: T) -> Self {
        Self {
            request_id: request_id.into(),
            trace_id: trace_id.into(),
            ok: true,
            data: Some(data),
            warnings: Vec::new(),
            provenance: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationType {
    Obligation,
    Epistemic,
    Relational,
    #[default]
    Narrative,
    Risk,
    Opportunity,
    Rhythm,
}

impl FromStr for ContinuationType {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        serde_json::from_value(serde_json::Value::String(value.to_string()))
            .map_err(|_| format!("invalid continuation type: {value}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationStatus {
    Active,
    Stabilized,
    Deferred,
    Closed,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContinuationInput {
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub continuation_type: ContinuationType,
    pub status: ContinuationStatus,
    pub parent_id: Option<String>,
    pub raw_event_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StoredContinuation {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub continuation_type: ContinuationType,
    pub status: ContinuationStatus,
    pub parent_id: Option<String>,
    pub raw_event_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationRelationKind {
    Supports,
    Conflicts,
    Blocks,
    DependsOn,
    Subsumes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContinuationRelationEdge {
    pub from_id: String,
    pub to_id: String,
    pub kind: ContinuationRelationKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LensRequest {
    pub query: String,
    pub horizon: Vec<String>,
    pub perspective: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LensCard {
    pub stance: String,
    pub why_now: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_continuations: Vec<LensActiveContinuation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commitment_constraints: Vec<StoredCommitment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boundaries: Vec<LensBoundary>,
    pub avoid: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_action: Option<LensPreferredAction>,
    pub open_questions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temporal_debt: Option<TemporalDebtWarning>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationDelta {
    Create,
    Activate,
    Advance,
    Stabilize,
    Defer,
    Renegotiate,
    Repair,
    Verify,
    Close,
    Retire,
    Split,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LensActiveContinuation {
    pub id: String,
    pub title: String,
    pub continuation_type: ContinuationType,
    pub pressure: f64,
    pub activation: f64,
    pub risk_if_ignored: String,
    pub recommended_delta: ContinuationDelta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LensBoundary {
    pub kind: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LensPreferredAction {
    pub name: String,
    pub reason: String,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TemporalDebtWarning {
    pub score: f64,
    pub level: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CandidateAction {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_id: Option<String>,
    pub progress: f64,
    pub closure: f64,
    pub option_value_preserved: f64,
    pub risk: f64,
    pub irreversibility: f64,
    pub confusion: f64,
    pub friction: f64,
    pub temporal_debt_added: f64,
    pub uncertainty: f64,
    pub externality: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ForecastRequest {
    pub actions: Vec<CandidateAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<ContinuationRelationEdge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RankedAction {
    pub name: String,
    pub score: f64,
    pub requires_confirmation: bool,
    pub ask_before_act: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ForecastResult {
    pub ranked_actions: Vec<RankedAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PreflightSignals {
    pub uncertainty: f64,
    pub irreversibility: f64,
    pub externality: f64,
    pub option_value_loss: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PreflightResult {
    pub requires_confirmation: bool,
    pub risk_product: f64,
    pub threshold: f64,
    pub reason: String,
    pub safer_alternative: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CommitRequest {
    pub speaker: String,
    pub statement: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
    pub revocable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StoredCommitment {
    pub id: String,
    pub continuation_id: String,
    pub speaker: String,
    pub statement: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
    pub revocable: bool,
    pub status: ContinuationStatus,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContinuationStatusDelta {
    pub continuation_id: String,
    pub delta: ContinuationDelta,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TemporalClaim {
    pub claim: String,
    pub evidence_status: EvidenceStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TemporalEvidenceRef {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TemporalDeltaInput {
    pub action_id: String,
    pub changes: Vec<ContinuationStatusDelta>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claims_made: Vec<TemporalClaim>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<TemporalEvidenceRef>,
}
