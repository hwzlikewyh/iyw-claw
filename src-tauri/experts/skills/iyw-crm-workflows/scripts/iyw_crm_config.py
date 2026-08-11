from __future__ import annotations

import json
import os
import stat
from pathlib import Path
from typing import Any

CONFIG_ENV = "IYW_CRM_CONFIG_DIR"
ALLOWED_FIELDS = frozenset({"version", "username", "password", "cookies"})
SECRET_KEYS = frozenset(
    {
        "authorization",
        "cookie",
        "cookies",
        "password",
        "requestverificationtoken",
        "token",
        "username",
    }
)


def resolve_config_dir(explicit: str | Path | None = None) -> Path:
    if explicit:
        return Path(explicit).expanduser()
    configured = os.getenv(CONFIG_ENV)
    if configured:
        return Path(configured).expanduser()
    return Path.home() / ".iyw-claw" / "iyw-crm-workflows"


def _normalized_key(key: str) -> str:
    return key.replace("_", "").replace("-", "").lower()


def public_data(value: Any) -> Any:
    if isinstance(value, list):
        return [public_data(item) for item in value]
    if not isinstance(value, dict):
        return value
    result: dict[str, Any] = {}
    for key, item in value.items():
        normalized = _normalized_key(str(key))
        is_secret = normalized in SECRET_KEYS or "token" in normalized
        result[key] = "<redacted>" if is_secret else public_data(item)
    return result


class SessionStore:
    def __init__(self, directory: str | Path | None = None):
        self.directory = resolve_config_dir(directory)
        self.path = self.directory / "session.json"

    def load(self) -> dict[str, Any]:
        if not self.path.exists():
            return {"version": 1}
        try:
            data = json.loads(self.path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            raise ValueError(f"invalid CRM session file: {self.path}") from exc
        if not isinstance(data, dict):
            raise TypeError(f"CRM session file must contain an object: {self.path}")
        unsupported = set(data) - ALLOWED_FIELDS
        if unsupported:
            names = ", ".join(sorted(unsupported))
            raise ValueError(f"CRM session file contains unsupported fields: {names}")
        if data.get("username") is not None and not isinstance(data["username"], str):
            raise TypeError("CRM session username must be a string")
        if data.get("password") is not None and not isinstance(data["password"], str):
            raise TypeError("CRM session password must be a string")
        if data.get("cookies") is not None and not isinstance(data["cookies"], list):
            raise TypeError("CRM session cookies must be a list")
        return data

    def update(self, **values: Any) -> dict[str, Any]:
        unsupported = set(values) - ALLOWED_FIELDS
        if unsupported:
            names = ", ".join(sorted(unsupported))
            raise ValueError(f"refusing to save unsupported session fields: {names}")
        data = self.load()
        data.update({key: value for key, value in values.items() if value is not None})
        data["version"] = 1
        self._write(data)
        return data

    def saved_credentials(self) -> tuple[str | None, str | None]:
        data = self.load()
        username = data.get("username")
        password = data.get("password")
        return (
            username if isinstance(username, str) and username else None,
            password if isinstance(password, str) and password else None,
        )

    def discard(self, *fields: str) -> bool:
        unsupported = set(fields) - ALLOWED_FIELDS
        if unsupported:
            names = ", ".join(sorted(unsupported))
            raise ValueError(f"refusing to remove unsupported session fields: {names}")
        data = self.load()
        removed = False
        for field in fields:
            if field in data:
                del data[field]
                removed = True
        if removed:
            self._write(data)
        return removed

    def invalidate_saved_credentials(self) -> bool:
        return self.discard("password", "cookies")

    def summary(self) -> dict[str, Any]:
        data = self.load()
        cookies = data.get("cookies") if isinstance(data.get("cookies"), list) else []
        has_saved_account = bool(data.get("username"))
        return {
            "path": str(self.path),
            "configured": self.path.exists(),
            "has_username": has_saved_account,
            "has_saved_account": has_saved_account,
            "has_saved_credentials": bool(
                has_saved_account and data.get("password")
            ),
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
        _restrict_permissions(temporary)
        os.replace(temporary, self.path)
        _restrict_permissions(self.path)


def _restrict_permissions(path: Path) -> None:
    try:
        os.chmod(path, stat.S_IRUSR | stat.S_IWUSR)
    except OSError:
        pass
