from __future__ import annotations

import json
import sys
from typing import Any

RISK_THRESHOLD = 0.65
RIVER_MODEL = "tfk-river-sidecar-v0"
HEURISTIC_MODEL = "tfk-heuristic-sidecar-v0"


class _HeuristicRiskScorer:
    model = HEURISTIC_MODEL
    degraded = True

    def __init__(
        self,
        reason: str = "river unavailable; deterministic heuristic scorer active",
    ) -> None:
        self.reason = reason

    def score(self, action: dict[str, Any]) -> float:
        return _heuristic_risk_score(action)


class _RiverRiskScorer:
    model = RIVER_MODEL
    degraded = False
    reason = ""

    def __init__(self, stats_module: Any) -> None:
        self._mean = stats_module.Mean()

    def score(self, action: dict[str, Any]) -> float:
        score = _heuristic_risk_score(action)
        # Keep a tiny River-backed online statistic so the optional sidecar
        # exercises River when present without making forecasts depend on it.
        try:
            self._mean.update(score)
        except Exception:
            pass
        return score


def _build_scorer() -> _HeuristicRiskScorer | _RiverRiskScorer:
    try:
        from river import stats
    except Exception:
        return _HeuristicRiskScorer()

    try:
        return _RiverRiskScorer(stats)
    except Exception as error:
        return _HeuristicRiskScorer(
            f"river unavailable; deterministic heuristic scorer active ({error})"
        )


_SCORER = _build_scorer()


def _clamped_float(value: Any, default: float = 0.0) -> float:
    try:
        number = float(value)
    except (TypeError, ValueError):
        return default
    return max(0.0, min(1.0, number))


def _heuristic_risk_score(action: dict[str, Any]) -> float:
    risk = _clamped_float(action.get("risk"))
    irreversibility = _clamped_float(action.get("irreversibility"))
    temporal_debt = _clamped_float(action.get("temporal_debt_added"))
    uncertainty = _clamped_float(action.get("uncertainty"))
    externality = _clamped_float(action.get("externality"))
    option_loss = 1.0 - _clamped_float(action.get("option_value_preserved"))

    return _clamped_float(
        (0.24 * risk)
        + (0.18 * irreversibility)
        + (0.18 * temporal_debt)
        + (0.16 * uncertainty)
        + (0.16 * externality)
        + (0.08 * option_loss)
    )


def _risk_reason(action: dict[str, Any]) -> str:
    fields = [
        "risk",
        "irreversibility",
        "temporal_debt_added",
        "uncertainty",
        "externality",
        "option_value_preserved",
    ]
    parts = [f"{field}={_clamped_float(action.get(field)):.2f}" for field in fields]
    return "forming future risk from CandidateAction fields: " + ", ".join(parts)


def _forecast_payload(request: dict[str, Any]) -> dict[str, Any]:
    nested = request.get("request")
    if isinstance(nested, dict):
        return nested
    return request


def _actions(payload: dict[str, Any]) -> list[Any]:
    actions = payload.get("actions", [])
    if isinstance(actions, list):
        return actions
    return []


def _advisory_signals(payload: dict[str, Any]) -> list[dict[str, Any]]:
    signals: list[dict[str, Any]] = []
    for action in _actions(payload):
        if not isinstance(action, dict):
            continue
        confidence = _SCORER.score(action)
        if confidence < RISK_THRESHOLD:
            continue

        signal: dict[str, Any] = {
            "name": "forming_future_risk",
            "model": _SCORER.model,
            "confidence": round(confidence, 3),
            "reason": _risk_reason(action),
        }
        action_name = action.get("name")
        if action_name is not None:
            signal["action_name"] = str(action_name)
        signals.append(signal)
    return signals


def handle(request: dict[str, Any]) -> dict[str, Any]:
    response = {
        "request_id": request.get("request_id", "unknown"),
        "predictions": [],
        "advisory_signals": _advisory_signals(_forecast_payload(request)),
        "degraded": _SCORER.degraded,
        "reason": _SCORER.reason if _SCORER.degraded else "",
    }
    return response


def main() -> None:
    for line in sys.stdin:
        if not line.strip():
            continue
        request = json.loads(line)
        print(json.dumps(handle(request), ensure_ascii=False), flush=True)


if __name__ == "__main__":
    main()
