#!/usr/bin/env python3
from __future__ import annotations

import argparse
import getpass
import json
import sys
from typing import Any

from iyw_crm_client import (
    DEFAULT_CRM_BASE_URL,
    FUSION_API_BASE_URL,
    CrmClient,
    CrmError,
)
from iyw_crm_config import SessionStore

MAX_ROWS = 200
SEARCH_DEFAULTS = {
    "Star": "0",
    "IsSysFenpei": "False",
    "BelongId": "-3",
    "Condition": "1",
    "ConditionText": "",
    "IndustryType": "0",
    "ConditionEx": "21",
    "ConditionDropDownEx": "-1",
    "ShareCustomerType1": "-1",
    "ShareCustomerType2": "-1",
    "ShareCustomerType3": "-1",
    "LockCustomerType": "-1",
    "CustomerSource": "",
    "SearchTime": "1",
    "startime": "",
    "endtime": "",
    "IsDownApp": "-1",
    "IsImportant": "false",
    "IsMonthActive": "-1",
    "BusinessModel": "-1",
    "BusinessModelValue": "-1",
    "CustomerNumTypeValue": "-1",
    "CustomerNumType": "-1",
    "IsIp": "-1",
    "IsDesigner": "-1",
    "IsHaveAlone": "-1",
    "ConditionDropDownTag": "-1",
    "ConditionDropDownTag2": "",
    "page": "1",
    "rows": "15",
}


def build_customer_search(
    text: str, *, page: int, rows: int, overrides: list[str] | None
) -> dict[str, str]:
    if page < 1:
        raise ValueError("page must be at least 1")
    if not 1 <= rows <= MAX_ROWS:
        raise ValueError(f"rows must be between 1 and {MAX_ROWS}")
    result = dict(SEARCH_DEFAULTS)
    for item in overrides or []:
        if "=" not in item:
            raise ValueError(f"field must be KEY=VALUE: {item}")
        key, value = item.split("=", 1)
        if key not in SEARCH_DEFAULTS:
            raise ValueError(f"unknown customer search field: {key}")
        result[key] = value
    result.update({"ConditionText": text, "page": str(page), "rows": str(rows)})
    return result


def operation_catalog() -> list[dict[str, Any]]:
    return [
        {
            "operation": "customer-search",
            "method": "POST",
            "path": "/Customer",
            "description": "Search CRM customers with captured filters",
        }
    ]


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="CLI for captured IYW CRM workflows")
    parser.add_argument("--config-dir")
    parser.add_argument("--base-url", default=DEFAULT_CRM_BASE_URL)
    parser.add_argument("--timeout", type=float, default=30)
    parser.add_argument("--allow-insecure-http", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    subparsers = parser.add_subparsers(dest="command", required=True)

    auth = subparsers.add_parser("auth", help="manage the local CRM session")
    auth_actions = auth.add_subparsers(dest="auth_action", required=True)
    auth_actions.add_parser("status", help="show non-secret session state")
    login = auth_actions.add_parser("login", help="log in using hidden password input")
    login.add_argument(
        "--interactive",
        action="store_true",
        help="prompt for a fresh username instead of reusing the saved one",
    )
    login.add_argument("--username", help="username paired with --password")
    login.add_argument("--password", help="password paired with --username")
    auth_actions.add_parser("ensure", help="verify the saved CRM session")
    auth_actions.add_parser("logout", help="remove the saved CRM session")

    api = subparsers.add_parser("api", help="call a captured CRM operation")
    operations = api.add_subparsers(dest="operation", required=True)
    operations.add_parser("list", help="list captured CRM operations")
    search = operations.add_parser("customer-search", help="search CRM customers")
    search.add_argument("--text", default="", help="customer search text")
    search.add_argument("--page", type=int, default=1)
    search.add_argument("--rows", type=int, default=15)
    search.add_argument("--field", action="append", default=[], metavar="KEY=VALUE")
    return parser


def _client(args: argparse.Namespace, store: SessionStore) -> CrmClient:
    return CrmClient(
        store,
        timeout=args.timeout,
        base_url=args.base_url,
        allow_insecure_http=args.allow_insecure_http or args.dry_run,
        load_session=not args.dry_run,
    )


def _run_auth(args: argparse.Namespace, store: SessionStore) -> Any:
    if args.auth_action == "status":
        return {
            **store.summary(),
            "crm_base_url": args.base_url,
            "fusion_api_base_url": FUSION_API_BASE_URL,
        }
    if args.dry_run:
        action = "auth-login" if args.auth_action == "login" else "auth-ensure"
        if args.auth_action == "logout":
            action = "auth-logout"
        return {
            "operation": action,
            "base_url": args.base_url,
            "network": False,
            "password_read": False,
        }
    if args.auth_action == "logout":
        return {"removed": store.clear(), "path": str(store.path)}
    client = _client(args, store)
    if args.auth_action == "ensure":
        return client.ensure_authenticated()
    username = str(args.username or "")
    password = str(args.password or "")
    if bool(username) != bool(password):
        raise ValueError("--username and --password must be provided together")
    if not username:
        saved = store.load()
        username = "" if args.interactive else str(saved.get("username") or "")
        if not username:
            username = input("CRM username: ").strip()
        if not username:
            raise ValueError("CRM username must not be empty")
        password = getpass.getpass("CRM password: ")
        if not password:
            raise ValueError("CRM password must not be empty")
    try:
        return client.login(username, password)
    finally:
        password = ""


def _run_api(args: argparse.Namespace, store: SessionStore) -> Any:
    if args.operation == "list":
        return operation_catalog()
    form = build_customer_search(
        args.text, page=args.page, rows=args.rows, overrides=args.field
    )
    return _client(args, store).search_customers(form, dry_run=args.dry_run)


def _success(data: Any) -> int:
    print(json.dumps({"ok": True, "data": data}, ensure_ascii=False, indent=2))
    return 0


def _failure(exc: Exception) -> int:
    code = exc.code if isinstance(exc, CrmError) else "invalid_input"
    retryable = exc.retryable if isinstance(exc, CrmError) else False
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


def _configure_stdout() -> None:
    reconfigure = getattr(sys.stdout, "reconfigure", None)
    if callable(reconfigure):
        reconfigure(encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    _configure_stdout()
    args = build_parser().parse_args(argv)
    store = SessionStore(args.config_dir)
    try:
        if args.command == "auth":
            return _success(_run_auth(args, store))
        return _success(_run_api(args, store))
    except (CrmError, OSError, TypeError, ValueError) as exc:
        return _failure(exc)
    except (EOFError, KeyboardInterrupt):
        return _failure(ValueError("credential input cancelled"))


if __name__ == "__main__":
    raise SystemExit(main())
