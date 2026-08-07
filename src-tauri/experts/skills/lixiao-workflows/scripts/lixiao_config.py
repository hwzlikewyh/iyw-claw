from __future__ import annotations

import json
import os
import stat
from pathlib import Path
from typing import Any


IYW_ACCOUNT_TOKEN_FILENAME = "iyw-account-token.json"
ALLOWED_FIELDS = frozenset(
    {
        "version",
        "app_token",
        "access_token",
        "business_token",
        "ttocr_token",
        "ttocr_token_info",
        # Legacy names remain readable for existing credential files.
        "login_token",
        "token_info",
        "refresh_token",
        "phone",
        "password",
        "cookies",
    }
)
REDACTED_KEYS = frozenset(
    {
        "appkey",
        "appsecret",
        "authorization",
        "cookie",
        "cookies",
        "password",
        "phone",
        "ticket",
    }
)


def load_iyw_account_access_token(path: str | Path | None = None) -> str:
    token_path = (
        Path(path).expanduser()
        if path is not None
        else Path.home() / ".iyw-claw" / IYW_ACCOUNT_TOKEN_FILENAME
    )
    try:
        raw = token_path.read_text(encoding="utf-8")
    except FileNotFoundError:
        return ""
    except OSError as exc:
        raise ValueError(f"failed to read IYW account token file: {token_path}") from exc
    try:
        data = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise ValueError(
            f"IYW account token file contains invalid JSON: {token_path}"
        ) from exc
    if not isinstance(data, dict):
        raise ValueError(
            f"IYW account token file must contain a JSON object: {token_path}"
        )
    access_token = data.get("access_token")
    return access_token.strip() if isinstance(access_token, str) else ""


def resolve_config_dir(explicit: str | Path | None = None) -> Path:
    if explicit:
        return Path(explicit).expanduser()
    configured = os.getenv("LIXIAO_CONFIG_DIR")
    return Path(configured).expanduser() if configured else Path.home() / ".iyw-claw"


def _normalized_key(key: str) -> str:
    return key.replace("_", "").replace("-", "").lower()


def _is_secret_key(key: str) -> bool:
    normalized = _normalized_key(key)
    if normalized.startswith("has") and "token" in normalized:
        return False
    return "token" in normalized or normalized in REDACTED_KEYS


def public_data(value: Any) -> Any:
    if isinstance(value, list):
        return [public_data(item) for item in value]
    if not isinstance(value, dict):
        return value
    result: dict[str, Any] = {}
    for key, item in value.items():
        result[key] = "<redacted>" if _is_secret_key(key) else public_data(item)
    return result


class CredentialStore:
    def __init__(self, directory: str | Path | None = None):
        self.directory = resolve_config_dir(directory)
        self.path = self.directory / "credentials.json"

    def load(self) -> dict[str, Any]:
        if not self.path.exists():
            return {"version": 1}
        data = json.loads(self.path.read_text(encoding="utf-8"))
        if not isinstance(data, dict):
            raise ValueError(f"invalid credential file: {self.path}")
        return data

    def update(self, **values: Any) -> dict[str, Any]:
        unsupported = set(values) - ALLOWED_FIELDS
        if unsupported:
            names = ", ".join(sorted(unsupported))
            raise ValueError(f"refusing to save unsupported credential fields: {names}")
        data = self.load()
        data.update({key: value for key, value in values.items() if value is not None})
        data["version"] = 1
        self._write(data)
        return data

    def summary(self) -> dict[str, Any]:
        data = self.load()
        cookies = data.get("cookies") if isinstance(data.get("cookies"), list) else []
        has_iyw_token = bool(
            load_iyw_account_access_token()
            or os.getenv("IYW_TOKEN")
            or os.getenv("LIXIAO_TTOCR_TOKEN")
            or data.get("ttocr_token")
            or data.get("login_token")
        )
        return {
            "path": str(self.path),
            "configured": self.path.exists(),
            "has_app_token": bool(data.get("app_token")),
            "has_access_token": bool(data.get("access_token")),
            "has_business_token": bool(data.get("business_token")),
            "has_iyw_token": has_iyw_token,
            "has_ttocr_token": has_iyw_token,
            "has_account": bool(data.get("phone") and data.get("password")),
            "cookie_count": len(cookies),
        }

    def clear(self) -> bool:
        if not self.path.exists():
            return False
        self.path.unlink()
        return True

    def _write(self, data: dict[str, Any]) -> None:
        self.directory.mkdir(parents=True, exist_ok=True)
        temporary = self.path.with_name(f".{self.path.name}.{os.getpid()}.tmp")
        payload = json.dumps(data, ensure_ascii=False, indent=2) + "\n"
        temporary.write_text(payload, encoding="utf-8")
        os.chmod(temporary, stat.S_IRUSR | stat.S_IWUSR)
        os.replace(temporary, self.path)
        os.chmod(self.path, stat.S_IRUSR | stat.S_IWUSR)
