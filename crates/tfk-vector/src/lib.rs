use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorDocumentKind {
    RawEvent,
    Continuation,
    AdvisoryForecastSignal,
}

impl VectorDocumentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RawEvent => "raw_event",
            Self::Continuation => "continuation",
            Self::AdvisoryForecastSignal => "advisory_forecast_signal",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorDocument {
    pub source_id: String,
    pub kind: VectorDocumentKind,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
}

impl VectorDocument {
    pub fn raw_event(source_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            source_id: source_id.into(),
            kind: VectorDocumentKind::RawEvent,
            text: text.into(),
            embedding: None,
        }
    }

    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorHit {
    pub source_id: String,
    pub kind: VectorDocumentKind,
    pub distance: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum VectorIndexStatus {
    Available { backend: String },
    Unavailable { backend: String, reason: String },
}

impl VectorIndexStatus {
    pub fn available(backend: impl Into<String>) -> Self {
        Self::Available {
            backend: backend.into(),
        }
    }

    pub fn unavailable(backend: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Unavailable {
            backend: backend.into(),
            reason: reason.into(),
        }
    }

    pub fn backend(&self) -> &str {
        match self {
            Self::Available { backend } | Self::Unavailable { backend, .. } => backend,
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum VectorIndexOutcome {
    Indexed,
    Skipped { reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum VectorError {
    #[error("vector embedding is required for backend {backend}")]
    MissingEmbedding { backend: String },
    #[error("vector embedding has {actual} dimensions; expected {expected}")]
    DimensionMismatch { expected: usize, actual: usize },
    #[error("sqlite-vec unavailable: {0}")]
    SqliteVecUnavailable(String),
    #[error("vector backend failed: {0}")]
    Backend(String),
    #[cfg(feature = "sqlite-vec")]
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, VectorError>;

pub trait VectorIndex: std::fmt::Debug + Send + Sync {
    fn status(&self) -> VectorIndexStatus;
    fn upsert(&self, document: &VectorDocument) -> Result<VectorIndexOutcome>;
    fn search(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<VectorHit>>;
}

#[derive(Debug, Clone)]
pub struct NoopVectorIndex {
    reason: String,
}

impl Default for NoopVectorIndex {
    fn default() -> Self {
        Self::new("vector backend is not configured")
    }
}

impl NoopVectorIndex {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl VectorIndex for NoopVectorIndex {
    fn status(&self) -> VectorIndexStatus {
        VectorIndexStatus::unavailable("noop", self.reason.clone())
    }

    fn upsert(&self, _document: &VectorDocument) -> Result<VectorIndexOutcome> {
        Ok(VectorIndexOutcome::Skipped {
            reason: self.reason.clone(),
        })
    }

    fn search(&self, _query_embedding: &[f32], _limit: usize) -> Result<Vec<VectorHit>> {
        Ok(Vec::new())
    }
}

pub fn validate_embedding_dimensions(embedding: &[f32], dimensions: usize) -> Result<()> {
    if embedding.len() == dimensions {
        Ok(())
    } else {
        Err(VectorError::DimensionMismatch {
            expected: dimensions,
            actual: embedding.len(),
        })
    }
}

#[cfg(not(feature = "sqlite-vec"))]
pub fn sqlite_vec_status() -> VectorIndexStatus {
    VectorIndexStatus::unavailable("sqlite-vec", "compiled without sqlite-vec feature")
}

#[cfg(feature = "sqlite-vec")]
pub fn sqlite_vec_status() -> VectorIndexStatus {
    let conn = match rusqlite::Connection::open_in_memory() {
        Ok(conn) => conn,
        Err(error) => {
            return VectorIndexStatus::unavailable("sqlite-vec", error.to_string());
        }
    };
    sqlite_vec_status_for_connection(&conn, 1)
}

#[cfg(feature = "sqlite-vec")]
pub fn sqlite_vec_status_for_connection(
    conn: &rusqlite::Connection,
    dimensions: usize,
) -> VectorIndexStatus {
    let dimensions = dimensions.max(1);
    let sql = format!(
        "CREATE VIRTUAL TABLE temp.tfk_vec_probe USING vec0(embedding float[{dimensions}])"
    );
    match conn.execute(&sql, []) {
        Ok(_) => {
            let _ = conn.execute("DROP TABLE temp.tfk_vec_probe", []);
            VectorIndexStatus::available("sqlite-vec")
        }
        Err(error) => VectorIndexStatus::unavailable("sqlite-vec", error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_index_is_explicitly_unavailable_and_skips_documents() {
        let index = NoopVectorIndex::new("missing sqlite-vec runtime");
        assert_eq!(
            index.status(),
            VectorIndexStatus::unavailable("noop", "missing sqlite-vec runtime")
        );
        assert_eq!(
            index
                .upsert(&VectorDocument::raw_event("evt_1", "temporal field"))
                .unwrap(),
            VectorIndexOutcome::Skipped {
                reason: "missing sqlite-vec runtime".to_string()
            }
        );
        assert!(index.search(&[0.1, 0.2], 10).unwrap().is_empty());
    }

    #[test]
    fn embedding_dimension_validation_is_strict() {
        assert!(validate_embedding_dimensions(&[0.1, 0.2], 2).is_ok());
        assert!(matches!(
            validate_embedding_dimensions(&[0.1], 2),
            Err(VectorError::DimensionMismatch {
                expected: 2,
                actual: 1
            })
        ));
    }

    #[test]
    fn sqlite_vec_status_reports_feature_contract() {
        let status = sqlite_vec_status();
        #[cfg(not(feature = "sqlite-vec"))]
        assert_eq!(
            status,
            VectorIndexStatus::unavailable("sqlite-vec", "compiled without sqlite-vec feature")
        );
        #[cfg(feature = "sqlite-vec")]
        assert_eq!(status.backend(), "sqlite-vec");
    }
}
