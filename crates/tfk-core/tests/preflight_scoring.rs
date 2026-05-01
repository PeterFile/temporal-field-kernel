use tfk_core::{PreflightScorer, PreflightSignals};

#[test]
fn preflight_requires_confirmation_when_risk_product_crosses_threshold() {
    let scorer = PreflightScorer::with_threshold(0.20);
    let result = scorer.score(PreflightSignals {
        uncertainty: 0.7,
        irreversibility: 0.8,
        externality: 0.6,
        option_value_loss: 0.5,
    });

    assert!(result.requires_confirmation);
    assert_eq!(
        result.reason,
        "uncertainty * irreversibility * externality exceeds threshold"
    );
}

#[test]
fn preflight_does_not_block_low_irreversibility_actions() {
    let scorer = PreflightScorer::with_threshold(0.20);
    let result = scorer.score(PreflightSignals {
        uncertainty: 0.9,
        irreversibility: 0.1,
        externality: 0.9,
        option_value_loss: 0.2,
    });

    assert!(!result.requires_confirmation);
}
