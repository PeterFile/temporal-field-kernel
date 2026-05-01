use serde::{Deserialize, Serialize};

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
