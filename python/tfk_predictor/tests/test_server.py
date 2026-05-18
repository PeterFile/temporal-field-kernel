from __future__ import annotations

import pathlib
import sys
import tomllib
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

from tfk_predictor.server import handle


RIVER_MODEL = "tfk-river-sidecar-v0"
HEURISTIC_MODEL = "tfk-heuristic-sidecar-v0"
PYPROJECT = pathlib.Path(__file__).resolve().parents[1] / "pyproject.toml"


def candidate_action(**overrides):
    action = {
        "name": "verify then ship",
        "continuation_id": None,
        "progress": 0.5,
        "closure": 0.5,
        "option_value_preserved": 0.8,
        "risk": 0.1,
        "irreversibility": 0.1,
        "confusion": 0.1,
        "friction": 0.1,
        "temporal_debt_added": 0.0,
        "uncertainty": 0.2,
        "externality": 0.1,
    }
    action.update(overrides)
    return action


class ServerHandleTests(unittest.TestCase):
    def test_river_is_optional_packaging_dependency(self):
        pyproject = tomllib.loads(PYPROJECT.read_text())
        dependencies = pyproject["project"]["dependencies"]
        self.assertIn("optional-dependencies", pyproject["project"])
        optional_dependencies = pyproject["project"]["optional-dependencies"]

        self.assertIn("pydantic>=2", dependencies)
        self.assertFalse(
            any(dependency.startswith("river") for dependency in dependencies)
        )
        self.assertIn("river", optional_dependencies)
        self.assertIn("river>=0.22", optional_dependencies["river"])

    def assert_sidecar_model_status(self, response, signal):
        self.assertIn(signal["model"], {RIVER_MODEL, HEURISTIC_MODEL})
        if signal["model"] == HEURISTIC_MODEL:
            self.assertIs(response["degraded"], True)
            self.assertIn("river", response["reason"].lower())
        else:
            self.assertIs(response["degraded"], False)

    def test_risky_action_emits_forming_future_risk_signal(self):
        response = handle(
            {
                "request_id": "risk-1",
                "actions": [
                    candidate_action(
                        name="ship unverified API",
                        option_value_preserved=0.1,
                        risk=0.9,
                        irreversibility=0.85,
                        temporal_debt_added=0.8,
                        uncertainty=0.75,
                        externality=0.8,
                    )
                ],
                "relations": [],
            }
        )

        self.assertEqual(response["request_id"], "risk-1")
        self.assertEqual(response["predictions"], [])
        self.assertEqual(len(response["advisory_signals"]), 1)
        signal = response["advisory_signals"][0]
        self.assertEqual(signal["name"], "forming_future_risk")
        self.assertEqual(signal["action_name"], "ship unverified API")
        self.assertGreaterEqual(signal["confidence"], 0.65)
        self.assertLessEqual(signal["confidence"], 1.0)
        self.assertIsInstance(signal["reason"], str)
        self.assert_sidecar_model_status(response, signal)

    def test_low_risk_action_emits_no_advisory_signal(self):
        response = handle(
            {
                "request_id": "low-1",
                "actions": [candidate_action()],
                "relations": [],
            }
        )

        self.assertEqual(response["request_id"], "low-1")
        self.assertEqual(response["predictions"], [])
        self.assertEqual(response["advisory_signals"], [])
        self.assertIn("degraded", response)

    def test_forecast_fixture_wrapper_shape_is_accepted(self):
        response = handle(
            {
                "request_id": "wrapped-1",
                "request": {
                    "actions": [
                        candidate_action(
                            name="dangerous wrapped action",
                            option_value_preserved=0.0,
                            risk=0.8,
                            irreversibility=0.9,
                            temporal_debt_added=0.8,
                            uncertainty=0.8,
                            externality=0.7,
                        )
                    ],
                    "relations": [],
                },
            }
        )

        self.assertEqual(response["request_id"], "wrapped-1")
        self.assertEqual(response["predictions"], [])
        self.assertEqual(len(response["advisory_signals"]), 1)
        self.assertEqual(
            response["advisory_signals"][0]["action_name"],
            "dangerous wrapped action",
        )

    def test_old_empty_predictions_field_is_preserved(self):
        response = handle({"request_id": "empty", "actions": [], "relations": []})

        self.assertIn("predictions", response)
        self.assertEqual(response["predictions"], [])
        self.assertIsInstance(response["predictions"], list)
        self.assertEqual(response["advisory_signals"], [])


if __name__ == "__main__":
    unittest.main()
