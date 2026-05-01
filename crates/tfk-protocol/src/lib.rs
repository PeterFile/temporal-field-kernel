use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationType {
    Obligation,
    Epistemic,
    Relational,
    Narrative,
    Risk,
    Opportunity,
    Rhythm,
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
    pub avoid: Vec<String>,
    pub open_questions: Vec<String>,
}
