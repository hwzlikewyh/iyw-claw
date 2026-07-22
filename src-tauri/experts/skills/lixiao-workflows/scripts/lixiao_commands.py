from __future__ import annotations

import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, TextIO

from lixiao_catalog import SPECS, EndpointSpec


class CommandError(ValueError):
    pass


@dataclass(frozen=True, kw_only=True)
class ApiCall:
    operation: str
    endpoint: EndpointSpec
    query: dict[str, Any]
    body: dict[str, Any] | None


def build_call(
    operation: str,
    query: dict[str, Any] | None,
    body: dict[str, Any] | None,
) -> ApiCall:
    try:
        endpoint = SPECS[operation]
    except KeyError as exc:
        raise CommandError(f"unknown operation: {operation}") from exc
    merged = dict(endpoint.default_query)
    merged.update(query or {})
    missing = [key for key in endpoint.required_query if not merged.get(key)]
    if missing:
        raise CommandError(f"{operation} requires query fields: {', '.join(missing)}")
    if endpoint.body_required and body is None:
        raise CommandError(f"{operation} requires a JSON body")
    if endpoint.method == "GET" and body is not None:
        raise CommandError(f"{operation} does not accept a request body")
    return ApiCall(operation=operation, endpoint=endpoint, query=merged, body=body)


def parse_query_pairs(values: list[str] | None) -> dict[str, str]:
    result: dict[str, str] = {}
    for value in values or []:
        if "=" not in value:
            raise CommandError(f"query value must be KEY=VALUE: {value}")
        key, item = value.split("=", 1)
        if not key:
            raise CommandError("query key must not be empty")
        result[key] = item
    return result


def parse_json_input(value: str, stdin: TextIO | None = None) -> dict[str, Any]:
    if value == "-":
        raw = (stdin or sys.stdin).read()
    elif value.startswith("@"):
        raw = Path(value[1:]).read_text(encoding="utf-8")
    else:
        raw = value
    parsed = json.loads(raw)
    if not isinstance(parsed, dict):
        raise CommandError("JSON input must be an object")
    return parsed


def operation_catalog() -> list[dict[str, Any]]:
    return [
        {
            "operation": name,
            "method": spec.method,
            "service": spec.service,
            "path": spec.path,
            "auth": spec.auth,
            "required_query": list(spec.required_query),
            "body_required": spec.body_required,
            "description": spec.description,
        }
        for name, spec in SPECS.items()
    ]
