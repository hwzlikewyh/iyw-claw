#!/usr/bin/env python3
"""Build host-neutral and Claude Code plugin manifests."""

from __future__ import annotations

from typing import Any


SCHEMA_VERSION = 1
SUPPORTED_TARGETS = ("codex", "claude_code")


def display_name(plugin_name: str) -> str:
    return " ".join(part.capitalize() for part in plugin_name.split("-"))


def build_claude_plugin_json(plugin_name: str) -> dict[str, Any]:
    return {
        "name": plugin_name,
        "version": "0.1.0",
        "description": f"{display_name(plugin_name)} plugin",
        "author": {"name": "Local developer"},
    }


def build_iyw_plugin_json(plugin_name: str) -> dict[str, Any]:
    return {
        "schemaVersion": SCHEMA_VERSION,
        "name": plugin_name,
        "version": "0.1.0",
        "targets": list(SUPPORTED_TARGETS),
        "components": {
            "skills": [],
            "connectors": [],
        },
    }
