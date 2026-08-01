from __future__ import annotations

import json
import os
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.request import Request, build_opener

from lixiao_client import LixiaoError


DEFAULT_TTOCR_URL = "https://gateway.iyw.cn/iyw-fusion-api/v1/ttocr/recognize"


class CaptchaError(LixiaoError):
    code = "captcha_failed"

    def __init__(
        self, message: str, *, code: str | None = None, retryable: bool = True
    ):
        super().__init__(message, code=code, retryable=retryable)


def resolve_ttocr_url(explicit: str | None = None) -> str:
    if explicit:
        return explicit
    return os.getenv("LIXIAO_TTOCR_URL") or DEFAULT_TTOCR_URL


def solve_geetest(
    gt: str,
    challenge: str,
    *,
    url: str | None = None,
    timeout: float = 30,
    headers: dict[str, str] | None = None,
) -> dict[str, str]:
    if not gt or not challenge:
        raise CaptchaError("geetest gt/challenge missing", retryable=False)
    payload = json.dumps({"gt": gt, "challenge": challenge}).encode("utf-8")
    request = Request(
        resolve_ttocr_url(url),
        data=payload,
        method="POST",
        headers={"content-type": "application/json", **(headers or {})},
    )
    try:
        with build_opener().open(request, timeout=timeout) as response:
            raw = response.read()
    except HTTPError as exc:
        raw = exc.read()
    except (URLError, TimeoutError) as exc:
        raise CaptchaError(
            f"captcha service unavailable: {exc}", code="captcha_unavailable"
        ) from exc
    try:
        result = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise CaptchaError("captcha service returned invalid JSON") from exc
    return _extract_proof(result)


def _extract_proof(result: Any) -> dict[str, str]:
    candidates = [result]
    if isinstance(result, dict):
        for key in ("result", "data"):
            nested = result.get(key)
            if nested is not None:
                candidates.append(nested)
    for candidate in candidates:
        proof = _proof_from_candidate(candidate)
        if proof:
            return proof
    if isinstance(result, dict):
        message = result.get("msg") or result.get("message")
        if message:
            raise CaptchaError(f"captcha recognition failed: {message}")
    raise CaptchaError("captcha recognition failed: no validate in response")


def _proof_from_candidate(candidate: Any) -> dict[str, str] | None:
    if isinstance(candidate, str):
        return _proof_from_pipe(candidate)
    if not isinstance(candidate, dict):
        return None
    validate = candidate.get("validate") or candidate.get("geetest_validate")
    if not validate:
        return None
    validate = str(validate)
    seccode = candidate.get("seccode") or candidate.get("geetest_seccode")
    return {
        "validate": validate,
        "seccode": str(seccode or f"{validate}|jordan"),
        "challenge": str(candidate.get("challenge") or ""),
    }


def _proof_from_pipe(value: str) -> dict[str, str] | None:
    parts = [part.strip() for part in value.split("|")]
    if len(parts) != 2 or not all(parts):
        return None
    challenge, validate = parts
    return {
        "challenge": challenge,
        "validate": validate,
        "seccode": f"{validate}|jordan",
    }
