from __future__ import annotations

import json
import sys
from typing import Any


def handle(request: dict[str, Any]) -> dict[str, Any]:
    return {
        "request_id": request.get("request_id", "unknown"),
        "predictions": [],
        "degraded": True,
        "reason": "predictor scaffold: no model loaded",
    }


def main() -> None:
    for line in sys.stdin:
        if not line.strip():
            continue
        request = json.loads(line)
        print(json.dumps(handle(request), ensure_ascii=False), flush=True)


if __name__ == "__main__":
    main()
