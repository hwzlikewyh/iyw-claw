#!/usr/bin/env python3
"""Shared scaffold and validation rules for portable plugin manifests."""

from __future__ import annotations

import json
import re
from pathlib import Path, PurePosixPath
from typing import Any

from portable_manifest import SCHEMA_VERSION, SUPPORTED_TARGETS

SLUG_RE = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
SEMVER_RE = re.compile(
    r"^(0|[1-9]\d*)\."
    r"(0|[1-9]\d*)\."
    r"(0|[1-9]\d*)"
    r"(?:-(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)(?:\."
    r"(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*)?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
)


def validate_portable_plugin(
    plugin_root: Path,
    codex_manifest: dict[str, Any],
    errors: list[str],
) -> None:
    claude_manifest = _load_object(
        plugin_root / ".claude-plugin" / "plugin.json",
        ".claude-plugin/plugin.json",
        errors,
    )
    iyw_manifest = _load_object(
        plugin_root / ".iyw-plugin.json",
        ".iyw-plugin.json",
        errors,
    )
    if claude_manifest is None or iyw_manifest is None:
        return

    _reject_todo(claude_manifest, ".claude-plugin/plugin.json", errors)
    _reject_todo(iyw_manifest, ".iyw-plugin.json", errors)
    _validate_native_identity(plugin_root, codex_manifest, claude_manifest, errors)
    _validate_iyw_manifest(plugin_root, codex_manifest, iyw_manifest, errors)


def _load_object(path: Path, label: str, errors: list[str]) -> dict[str, Any] | None:
    if not path.is_file():
        errors.append(f"missing `{label}`")
        return None
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        errors.append(f"`{label}` must contain valid JSON")
        return None
    if not isinstance(payload, dict):
        errors.append(f"`{label}` must contain a JSON object")
        return None
    return payload


def _reject_todo(value: Any, path: str, errors: list[str]) -> None:
    if isinstance(value, str) and "[TODO:" in value:
        errors.append(f"{path} still contains a `[TODO: ...]` placeholder")
    elif isinstance(value, list):
        for index, item in enumerate(value):
            _reject_todo(item, f"{path}[{index}]", errors)
    elif isinstance(value, dict):
        for key, item in value.items():
            _reject_todo(item, f"{path}.{key}", errors)


def _validate_native_identity(
    plugin_root: Path,
    codex: dict[str, Any],
    claude: dict[str, Any],
    errors: list[str],
) -> None:
    allowed = {"name", "version", "description", "author", "homepage", "repository", "license", "keywords"}
    for key in sorted(set(claude) - allowed):
        errors.append(f".claude-plugin/plugin.json field `{key}` is not portable")

    name = _required_string(claude, "name", ".claude-plugin/plugin.json", errors)
    version = _required_string(claude, "version", ".claude-plugin/plugin.json", errors)
    _required_string(claude, "description", ".claude-plugin/plugin.json", errors)
    author = claude.get("author")
    if not isinstance(author, dict):
        errors.append(".claude-plugin/plugin.json field `author` must be an object")
    else:
        _required_string(author, "name", ".claude-plugin/plugin.json author", errors)

    codex_name = codex.get("name")
    codex_version = codex.get("version")
    if name is not None and name != plugin_root.name:
        errors.append("Claude plugin name must match the plugin directory name")
    if name is not None and name != codex_name:
        errors.append("Codex and Claude plugin names must match")
    if version is not None and SEMVER_RE.fullmatch(version) is None:
        errors.append(".claude-plugin/plugin.json field `version` must be strict semver")
    if version is not None and version != codex_version:
        errors.append("Codex and Claude plugin versions must match")


def _validate_iyw_manifest(
    plugin_root: Path,
    codex: dict[str, Any],
    manifest: dict[str, Any],
    errors: list[str],
) -> None:
    _reject_unknown(manifest, {"schemaVersion", "name", "version", "targets", "components"}, ".iyw-plugin.json", errors)
    if manifest.get("schemaVersion") != SCHEMA_VERSION:
        errors.append(f".iyw-plugin.json field `schemaVersion` must be {SCHEMA_VERSION}")
    name = _required_string(manifest, "name", ".iyw-plugin.json", errors)
    version = _required_string(manifest, "version", ".iyw-plugin.json", errors)
    if name is not None and name != codex.get("name"):
        errors.append(".iyw-plugin.json name must match the native plugin manifests")
    if version is not None and version != codex.get("version"):
        errors.append(".iyw-plugin.json version must match the native plugin manifests")

    targets = manifest.get("targets")
    if targets != list(SUPPORTED_TARGETS):
        errors.append(".iyw-plugin.json field `targets` must be [`codex`, `claude_code`]")

    components = manifest.get("components")
    if not isinstance(components, dict):
        errors.append(".iyw-plugin.json field `components` must be an object")
        return
    _reject_unknown(components, {"skills", "connectors"}, ".iyw-plugin.json components", errors)
    connector_keys, server_keys = _validate_connectors(
        plugin_root,
        codex,
        components.get("connectors"),
        errors,
    )
    _validate_skills(
        plugin_root,
        codex,
        components.get("skills"),
        connector_keys,
        errors,
    )
    if len(server_keys) != len(set(server_keys)):
        errors.append(".iyw-plugin.json connector `serverKey` values must be unique")


def _validate_connectors(
    plugin_root: Path,
    codex: dict[str, Any],
    entries: Any,
    errors: list[str],
) -> tuple[set[str], list[str]]:
    connector_keys: set[str] = set()
    server_keys: list[str] = []
    if not isinstance(entries, list):
        errors.append(".iyw-plugin.json field `components.connectors` must be an array")
        entries = []
    for index, entry in enumerate(entries):
        label = f".iyw-plugin.json components.connectors[{index}]"
        if not isinstance(entry, dict):
            errors.append(f"{label} must be an object")
            continue
        _reject_unknown(entry, {"key", "serverKey"}, label, errors)
        key = _component_key(entry.get("key"), f"{label}.key", errors)
        server_key = _component_key(entry.get("serverKey"), f"{label}.serverKey", errors)
        if key is not None and key in connector_keys:
            errors.append(f"duplicate connector key `{key}`")
        elif key is not None:
            connector_keys.add(key)
        if server_key is not None:
            server_keys.append(server_key)

    mcp_path = plugin_root / ".mcp.json"
    mcp_servers = _load_mcp_servers(mcp_path, errors) if mcp_path.is_file() else set()
    if server_keys and not mcp_path.is_file():
        errors.append("`.mcp.json` is required when connectors are declared")
    if set(server_keys) != mcp_servers:
        errors.append("connector `serverKey` values must exactly match `.mcp.json` servers")
    expected_mcp_path = "./.mcp.json" if mcp_path.is_file() else None
    if codex.get("mcpServers") != expected_mcp_path:
        errors.append("Codex `mcpServers` must reference `.mcp.json` exactly when it exists")
    return connector_keys, server_keys


def _validate_skills(
    plugin_root: Path,
    codex: dict[str, Any],
    entries: Any,
    connector_keys: set[str],
    errors: list[str],
) -> None:
    declared_keys: set[str] = set()
    declared_paths: set[str] = set()
    if not isinstance(entries, list):
        errors.append(".iyw-plugin.json field `components.skills` must be an array")
        entries = []
    for index, entry in enumerate(entries):
        label = f".iyw-plugin.json components.skills[{index}]"
        if not isinstance(entry, dict):
            errors.append(f"{label} must be an object")
            continue
        _reject_unknown(entry, {"key", "path", "requiresConnectors"}, label, errors)
        key = _component_key(entry.get("key"), f"{label}.key", errors)
        if key is not None and key in declared_keys:
            errors.append(f"duplicate skill key `{key}`")
        elif key is not None:
            declared_keys.add(key)
        path = _skill_path(plugin_root, entry.get("path"), f"{label}.path", errors)
        if path is not None and path in declared_paths:
            errors.append(f"duplicate skill path `{path}`")
        elif path is not None:
            declared_paths.add(path)
        required = entry.get("requiresConnectors", [])
        if not isinstance(required, list) or not all(isinstance(item, str) for item in required):
            errors.append(f"{label}.requiresConnectors must be an array of connector keys")
        elif len(required) != len(set(required)):
            errors.append(f"{label}.requiresConnectors must not contain duplicates")
        else:
            for connector_key in required:
                if connector_key not in connector_keys:
                    errors.append(f"{label} references unknown connector `{connector_key}`")

    skills_root = plugin_root / "skills"
    actual_paths = {
        f"skills/{path.name}"
        for path in skills_root.iterdir()
        if path.is_dir() and not path.name.startswith(".")
    } if skills_root.is_dir() else set()
    if declared_paths != actual_paths:
        errors.append("declared skill paths must exactly match directories under `skills/`")
    expected_skills_path = "./skills/" if skills_root.is_dir() else None
    if codex.get("skills") != expected_skills_path:
        errors.append("Codex `skills` must reference `./skills/` exactly when it exists")


def _skill_path(plugin_root: Path, value: Any, label: str, errors: list[str]) -> str | None:
    normalized = _relative_path(value)
    if normalized is None:
        errors.append(f"{label} must be a safe relative POSIX path")
        return None
    parts = PurePosixPath(normalized).parts
    if len(parts) != 2 or parts[0] != "skills":
        errors.append(f"{label} must point to one direct child of `skills/`")
        return None
    target = (plugin_root / normalized).resolve()
    if not target.is_relative_to(plugin_root.resolve()) or not (target / "SKILL.md").is_file():
        errors.append(f"{label} must point to a skill directory containing `SKILL.md`")
        return None
    return normalized


def _load_mcp_servers(path: Path, errors: list[str]) -> set[str]:
    payload = _load_object(path, ".mcp.json", errors)
    if payload is None:
        return set()
    if set(payload) != {"mcpServers"} or not isinstance(payload.get("mcpServers"), dict):
        errors.append("`.mcp.json` must contain only an object field named `mcpServers`")
        return set()
    return {key for key in payload["mcpServers"] if isinstance(key, str) and key.strip()}


def _component_key(value: Any, label: str, errors: list[str]) -> str | None:
    if not isinstance(value, str) or SLUG_RE.fullmatch(value) is None:
        errors.append(f"{label} must use lowercase hyphen-case")
        return None
    return value


def _relative_path(value: Any) -> str | None:
    if not isinstance(value, str) or not value.strip() or "\\" in value:
        return None
    candidate = PurePosixPath(value)
    if candidate.is_absolute() or any(part in {"", ".", ".."} for part in candidate.parts):
        return None
    if candidate.parts and ":" in candidate.parts[0]:
        return None
    return candidate.as_posix().rstrip("/")


def _required_string(
    payload: dict[str, Any],
    key: str,
    label: str,
    errors: list[str],
) -> str | None:
    value = payload.get(key)
    if not isinstance(value, str) or not value.strip():
        errors.append(f"{label} field `{key}` must be a non-empty string")
        return None
    return value

def _reject_unknown(
    payload: dict[str, Any],
    allowed: set[str],
    label: str,
    errors: list[str],
) -> None:
    for key in sorted(set(payload) - allowed):
        errors.append(f"{label} field `{key}` is not accepted by the portable contract")
