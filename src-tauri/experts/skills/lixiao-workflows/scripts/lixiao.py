#!/usr/bin/env python3
from __future__ import annotations

import argparse
import getpass
import json
import re
import sys
import time
from typing import Any

from lixiao_client import LixiaoClient, LixiaoError
from lixiao_commands import (
    SPECS,
    CommandError,
    build_call,
    operation_catalog,
    parse_json_input,
    parse_query_pairs,
)
from lixiao_config import CredentialStore, public_data, resolve_config_dir


DEFAULT_AGREEMENT = json.dumps(
    {"version": "2023-12-25 18:05", "service_version": "2022-09-15 03:30"},
    ensure_ascii=False,
)


def _flag_name(name: str) -> str:
    return "--" + re.sub(r"(?<!^)(?=[A-Z])", "-", name).replace("_", "-").lower()


def _add_api_parsers(subparsers: Any) -> None:
    api = subparsers.add_parser("api", help="call one of the captured API operations")
    operations = api.add_subparsers(dest="operation", required=True)
    operations.add_parser("list", help="list all captured operations")
    for name, spec in SPECS.items():
        command = operations.add_parser(name, help=spec.description)
        command.add_argument(
            "--query", action="append", default=[], metavar="KEY=VALUE"
        )
        command.add_argument("--body", help="inline JSON, @file.json, or - for stdin")
        for key in spec.required_query:
            command.add_argument(_flag_name(key), dest=f"required_{key}")


def _add_auth_parsers(subparsers: Any) -> None:
    auth = subparsers.add_parser("auth", help="manage login and saved credentials")
    actions = auth.add_subparsers(dest="auth_action", required=True)
    actions.add_parser("status", help="show credential status without secrets")
    actions.add_parser(
        "set-app-token", help="read and save the application token securely"
    )
    actions.add_parser(
        "set-business-token", help="read and save the business token securely"
    )
    actions.add_parser("qr-start", help="create a QR login code")
    wait = actions.add_parser(
        "qr-wait", help="poll a QR login code and save the session"
    )
    wait.add_argument("--code", required=True)
    wait.add_argument("--wait-seconds", type=int, default=120)
    wait.add_argument("--poll-interval", type=float, default=2)
    actions.add_parser("captcha", help="create a password-login captcha challenge")
    password = actions.add_parser(
        "password", help="log in without saving phone or password"
    )
    password.add_argument("--phone", required=True)
    password.add_argument("--challenge", required=True)
    password.add_argument("--validate", required=True)
    password.add_argument("--seccode", required=True)
    password.add_argument("--agreement-version", default=DEFAULT_AGREEMENT)
    actions.add_parser("app", help="refresh and save the application SSO token")
    actions.add_parser("logout", help="remove only the saved credentials file")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="CLI for captured Lixiao workflows")
    parser.add_argument("--config-dir")
    parser.add_argument("--timeout", type=float, default=30)
    parser.add_argument("--dry-run", action="store_true")
    subparsers = parser.add_subparsers(dest="command", required=True)
    _add_auth_parsers(subparsers)
    _add_api_parsers(subparsers)
    return parser


def _read_secret(label: str) -> str:
    if sys.stdin.isatty():
        value = getpass.getpass(f"{label}: ")
    else:
        value = sys.stdin.readline().strip()
    if not value:
        raise CommandError(f"{label} must not be empty")
    return value


def _execute_operation(
    client: LixiaoClient,
    operation: str,
    query: dict[str, Any] | None = None,
    *,
    body: dict[str, Any] | None = None,
    dry_run: bool = False,
) -> Any:
    return client.execute(build_call(operation, query or {}, body), dry_run=dry_run)


def _run_api(args: argparse.Namespace, client: LixiaoClient) -> Any:
    if args.operation == "list":
        return {"count": len(SPECS), "operations": operation_catalog()}
    query = parse_query_pairs(args.query)
    spec = SPECS[args.operation]
    for key in spec.required_query:
        value = getattr(args, f"required_{key}")
        if value:
            query[key] = value
    body = parse_json_input(args.body) if args.body is not None else None
    return _execute_operation(
        client, args.operation, query, body=body, dry_run=args.dry_run
    )


def _run_qr_wait(args: argparse.Namespace, client: LixiaoClient) -> Any:
    if args.wait_seconds < 0 or args.poll_interval <= 0:
        raise CommandError(
            "wait seconds must be non-negative and poll interval must be positive"
        )
    if args.dry_run:
        return _execute_operation(client, "qr-poll", {"code": args.code}, dry_run=True)
    deadline = time.monotonic() + args.wait_seconds
    while True:
        result = _execute_operation(client, "qr-poll", {"code": args.code})
        data = result.get("data") if isinstance(result, dict) else None
        if isinstance(data, dict) and data.get("status"):
            app = _execute_operation(client, "app-session")
            return {"status": "authenticated", "qr": result, "app": app}
        if time.monotonic() >= deadline:
            return {"status": "pending", "qr": result}
        time.sleep(args.poll_interval)


def _run_password(args: argparse.Namespace, client: LixiaoClient) -> Any:
    body = {
        "challenge": args.challenge,
        "validate": args.validate,
        "seccode": args.seccode,
        "password": _read_secret("Password"),
        "type": "login",
        "phone": args.phone,
        "agreementVersion": args.agreement_version,
    }
    login = _execute_operation(
        client, "password-login", body=body, dry_run=args.dry_run
    )
    if args.dry_run:
        return login
    app = _execute_operation(client, "app-session")
    return {"login": login, "app": app}


def _run_auth(
    args: argparse.Namespace, store: CredentialStore, client: LixiaoClient
) -> Any:
    action = args.auth_action
    if action == "status":
        return store.summary()
    if action in {"set-app-token", "set-business-token"}:
        field = "app_token" if action == "set-app-token" else "business_token"
        store.update(**{field: _read_secret(field.replace("_", " ").title())})
        return store.summary()
    if action == "logout":
        return {"removed": store.clear(), "path": str(store.path)}
    if action == "qr-start":
        return _execute_operation(client, "qr-start", dry_run=args.dry_run)
    if action == "qr-wait":
        return _run_qr_wait(args, client)
    if action == "captcha":
        return _execute_operation(client, "captcha-register", dry_run=args.dry_run)
    if action == "password":
        return _run_password(args, client)
    if action == "app":
        return _execute_operation(client, "app-session", dry_run=args.dry_run)
    raise CommandError(f"unsupported auth action: {action}")


def run(args: argparse.Namespace) -> Any:
    store = CredentialStore(resolve_config_dir(args.config_dir))
    client = LixiaoClient(store, timeout=args.timeout)
    if args.command == "auth":
        return _run_auth(args, store, client)
    return _run_api(args, client)


def main() -> int:
    args = build_parser().parse_args()
    try:
        result = run(args)
    except (
        LixiaoError,
        CommandError,
        ValueError,
        OSError,
        json.JSONDecodeError,
    ) as exc:
        code = getattr(exc, "code", "invalid_input")
        retryable = bool(getattr(exc, "retryable", False))
        print(
            json.dumps(
                {
                    "ok": False,
                    "error": {
                        "code": code,
                        "message": str(exc),
                        "retryable": retryable,
                    },
                },
                ensure_ascii=False,
                indent=2,
            )
        )
        return 1
    print(
        json.dumps(
            {"ok": True, "data": public_data(result)}, ensure_ascii=False, indent=2
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
