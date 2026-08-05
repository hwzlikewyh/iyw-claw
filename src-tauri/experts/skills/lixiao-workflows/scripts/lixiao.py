#!/usr/bin/env python3
from __future__ import annotations

import argparse
import getpass
import json
import re
import sys
import time
from typing import Any, Callable

from lixiao_advanced import (
    DEFAULT_PAGE_SIZE,
    build_advanced_search_body,
    build_channel_search_body,
    build_tender_search_body,
    empty_condition,
    response_items as advanced_response_items,
    validate_limits as validate_advanced_limits,
)
from lixiao_client import AuthenticationError, LixiaoClient, LixiaoError
from lixiao_commands import (
    SPECS,
    CommandError,
    build_call,
    operation_catalog,
    parse_json_input,
    parse_query_pairs,
)
from lixiao_config import CredentialStore, public_data, resolve_config_dir
from lixiao_crm_session import bootstrap_crm_session
from lixiao_ecommerce import build_search_body, resolve_platform, response_items
from lixiao_ttocr import resolve_ttocr_url, solve_geetest


DEFAULT_AGREEMENT = json.dumps(
    {"version": "2023-12-25 18:05", "service_version": "2022-09-15 03:30"},
    ensure_ascii=False,
)
DEFAULT_SEARCH_LIMIT = 100


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
        if name == "company-products":
            command.add_argument(
                "--unlock-if-needed",
                action="store_true",
                help="consume quota to unlock hidden products, then retry once",
            )
            command.add_argument(
                "--contact-source",
                help="source for contact lookup after an unlock",
            )


def _add_auth_parsers(subparsers: Any) -> None:
    auth = subparsers.add_parser("auth", help="manage login and saved credentials")
    actions = auth.add_subparsers(dest="auth_action", required=True)
    actions.add_parser("status", help="show credential status without secrets")
    login = actions.add_parser(
        "login",
        help="automated password login: captcha, ttocr solve, save account and session",
    )
    account_source = login.add_mutually_exclusive_group()
    account_source.add_argument(
        "--phone", help="login phone; defaults to the saved account"
    )
    account_source.add_argument(
        "--interactive",
        action="store_true",
        help="prompt for account and password, ignoring saved credentials",
    )
    login.add_argument(
        "--password", help="password paired with --phone for direct login"
    )
    login.add_argument("--ttocr-url", help="override the captcha gateway URL")
    ensure = actions.add_parser(
        "ensure", help="verify the saved session and re-login only when required"
    )
    ensure.add_argument("--ttocr-url", help="override the captcha gateway URL")
    actions.add_parser(
        "set-app-token", help="read and save the application token securely"
    )
    actions.add_parser(
        "set-business-token", help="read and save the business token securely"
    )
    actions.add_parser(
        "set-ttocr-token",
        help="save a legacy fallback token when IYW Claw login is unavailable",
    )
    actions.add_parser(
        "set-session-token", help="deprecated alias for set-ttocr-token"
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
        "password", help="log in with an externally solved Geetest proof"
    )
    password.add_argument("--phone", required=True)
    password.add_argument("--challenge", required=True)
    password.add_argument("--validate", required=True)
    password.add_argument("--seccode", required=True)
    password.add_argument("--agreement-version", default=DEFAULT_AGREEMENT)
    actions.add_parser("app", help="refresh and save the application SSO token")
    actions.add_parser("logout", help="remove only the saved credentials file")


def _add_workflow_parsers(subparsers: Any) -> None:
    workflow = subparsers.add_parser("workflow", help="run bundled business workflows")
    actions = workflow.add_subparsers(dest="workflow_action", required=True)
    search = actions.add_parser(
        "ecommerce-search", help="search ecommerce candidates without composing APIs"
    )
    search.add_argument("--keyword", required=True)
    search.add_argument("--platform", action="append", required=True)
    search.add_argument("--limit-per-platform", type=int, default=100)
    search.add_argument("--page-size", type=int, default=10)
    profile = actions.add_parser(
        "company-profile", help="collect complete profiles without composing APIs"
    )
    profile.add_argument("--id", action="append", required=True)
    profile.add_argument("--contact-source")
    _add_advanced_search_workflow_parsers(actions)


def _add_advanced_search_workflow_parsers(actions: Any) -> None:
    conditions = actions.add_parser(
        "search-conditions", help="list advanced enterprise search conditions"
    )
    conditions.add_argument("--group-name", default="enterprise")
    conditions.add_argument("--category", default="common.searchExhibitionNew.default")
    conditions.add_argument("--module", action="append")
    advanced = actions.add_parser(
        "advanced-search", help="search enterprises with captured advanced filters"
    )
    _add_paginated_search_args(advanced, condition_required=True, keyword=False)
    tender = actions.add_parser("tender-search", help="search tender projects")
    _add_paginated_search_args(tender, condition_required=False, keyword=True)
    channel = actions.add_parser("channel-search", help="search sales channels")
    _add_paginated_search_args(channel, condition_required=False, keyword=True)
    templates = actions.add_parser("search-templates", help="list saved search templates")
    templates.add_argument("--template-type", type=int, default=0)
    templates.add_argument("--page-size", type=int, default=20)
    templates.add_argument("--page-num", type=int, default=1)
    templates.add_argument("--search-name", default="")


def _add_paginated_search_args(
    command: Any, *, condition_required: bool, keyword: bool
) -> None:
    command.add_argument("--condition", required=condition_required)
    if keyword:
        command.add_argument("--keyword")
    command.add_argument("--limit", type=int, default=DEFAULT_SEARCH_LIMIT)
    command.add_argument("--page-size", type=int, default=DEFAULT_PAGE_SIZE)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="CLI for captured Lixiao workflows")
    parser.add_argument("--config-dir")
    parser.add_argument("--timeout", type=float, default=30)
    parser.add_argument("--dry-run", action="store_true")
    subparsers = parser.add_subparsers(dest="command", required=True)
    _add_auth_parsers(subparsers)
    _add_api_parsers(subparsers)
    _add_workflow_parsers(subparsers)
    return parser


def _read_secret(label: str) -> str:
    if sys.stdin.isatty():
        value = getpass.getpass(f"{label}: ")
    else:
        value = sys.stdin.readline().strip()
    if not value:
        raise CommandError(f"{label} must not be empty")
    return value


def _read_line(label: str) -> str:
    if sys.stdin.isatty():
        value = input(f"{label}: ")
    else:
        value = sys.stdin.readline().strip()
    if not value:
        raise CommandError(f"{label} must not be empty")
    return value


def _resolve_account(args: argparse.Namespace, store: CredentialStore) -> tuple[str, str]:
    direct_password = getattr(args, "password", None)
    if direct_password is not None:
        phone = getattr(args, "phone", None)
        if not phone or not direct_password or getattr(args, "interactive", False):
            raise CommandError("--password requires --phone and cannot use --interactive")
        return str(phone), str(direct_password)
    if getattr(args, "interactive", False):
        return _read_line("Login Phone/Account"), _read_secret("Password")

    saved = store.load()
    phone = getattr(args, "phone", None) or saved.get("phone")
    password = None
    if phone and phone == saved.get("phone"):
        password = saved.get("password")
    if not phone:
        phone = _read_line("Login Phone/Account")
    if not password:
        password = _read_secret("Password")
    return str(phone), password


def _execute_operation(
    client: LixiaoClient,
    operation: str,
    query: dict[str, Any] | None = None,
    *,
    body: dict[str, Any] | None = None,
    dry_run: bool = False,
) -> Any:
    return client.execute(build_call(operation, query or {}, body), dry_run=dry_run)


def _finish_password_login(client: LixiaoClient, login: Any) -> dict[str, Any]:
    crm = bootstrap_crm_session(client, login)
    app = _execute_operation(client, "app-session")
    return {"login": login, "crm": crm, "app": app}


def _auto_login(
    client: LixiaoClient,
    store: CredentialStore,
    *,
    phone: str,
    password: str,
    ttocr_url: str | None = None,
) -> Any:
    ttocr_headers = client.ttocr_headers()
    captcha = _execute_operation(client, "captcha-register")
    data = captcha.get("data") if isinstance(captcha, dict) else None
    gt = data.get("gt") if isinstance(data, dict) else None
    challenge = data.get("challenge") if isinstance(data, dict) else None
    if not gt or not challenge:
        raise CommandError("captcha register did not return gt and challenge")
    proof = solve_geetest(
        gt,
        challenge,
        url=ttocr_url,
        timeout=client.timeout,
        headers=ttocr_headers,
    )
    body = {
        "challenge": proof.get("challenge") or challenge,
        "validate": proof["validate"],
        "seccode": proof["seccode"],
        "password": password,
        "type": "login",
        "phone": phone,
        "agreementVersion": DEFAULT_AGREEMENT,
    }
    login = _execute_operation(client, "password-login", body=body)
    result = _finish_password_login(client, login)
    store.update(phone=phone, password=password)
    return result


def _saved_account(store: CredentialStore) -> tuple[str | None, str | None]:
    saved = store.load()
    return saved.get("phone"), saved.get("password")


def _products_require_unlock(result: Any) -> bool:
    data = result.get("data") if isinstance(result, dict) else None
    section = data.get("ShopGoodsInfo") if isinstance(data, dict) else None
    if not isinstance(section, dict):
        return False
    if section.get("enableView") is not None:
        return section.get("enableView") is False
    visible_keys = ("items", "list", "records", "rows", "products")
    return bool(section.get("total")) and not any(
        section.get(key) is not None for key in visible_keys
    )


def _products_view_available(result: Any) -> bool:
    data = result.get("data") if isinstance(result, dict) else None
    section = data.get("ShopGoodsInfo") if isinstance(data, dict) else None
    if not isinstance(section, dict):
        return False
    if isinstance(section.get("enableView"), bool):
        return bool(section["enableView"])
    visible_keys = ("items", "list", "records", "rows", "products")
    return any(section.get(key) is not None for key in visible_keys)


def _company_name(result: Any) -> str | None:
    data = result.get("data") if isinstance(result, dict) else None
    if not isinstance(data, dict):
        return None
    for key in ("entname", "entName", "companyName", "name"):
        value = data.get(key)
        if value:
            return str(value)
    return None


def _error_data(error: LixiaoError) -> dict[str, Any]:
    return {
        "code": error.code,
        "message": str(error),
        "retryable": error.retryable,
    }


def _contacts_after_unlock(
    client: LixiaoClient, entity_id: str, contact_source: str | None
) -> dict[str, Any]:
    try:
        company_card = _execute_operation(client, "company-card", {"id": entity_id})
        company_name = _company_name(company_card)
        if not company_name:
            raise LixiaoError(
                "company card response does not include an enterprise name",
                code="company_name_unavailable",
            )
        contact_query: dict[str, Any] = {
            "pid": entity_id,
            "entName": company_name,
        }
        if contact_source:
            contact_query["source"] = contact_source
        return {
            "performed": True,
            "company_name": company_name,
            "company_card": company_card,
            "contact_count": _execute_operation(
                client, "company-contacts-count", {"pid": entity_id}
            ),
            "contacts": _execute_operation(
                client, "company-contacts", contact_query
            ),
        }
    except LixiaoError as error:
        return {"performed": True, "error": _error_data(error)}


def _dry_run_contacts_after_unlock(
    client: LixiaoClient, entity_id: str, contact_source: str | None
) -> dict[str, Any]:
    return {
        "condition": "only after a real unlock request",
        "company_card": _execute_operation(
            client, "company-card", {"id": entity_id}, dry_run=True
        ),
        "contact_count": _execute_operation(
            client, "company-contacts-count", {"pid": entity_id}, dry_run=True
        ),
        "contacts": {
            "operation": "company-contacts",
            "entName": "resolved from company-card response",
            "source": contact_source
            or "scene_search.searchEcommercePlatformEnterprise_detail",
        },
    }


def _run_company_products(
    client: LixiaoClient,
    query: dict[str, Any],
    *,
    dry_run: bool,
    unlock_if_needed: bool,
    contact_source: str | None,
) -> Any:
    detail = _execute_operation(
        client, "company-products", query, dry_run=dry_run
    )
    if not unlock_if_needed:
        return detail
    entity_id = str(query.get("id") or "")
    if dry_run:
        unlock = _execute_operation(
            client, "company-unlock", {"entityId": entity_id}, dry_run=True
        )
        return {
            "condition": "unlock only when products are not viewable",
            "detail": detail,
            "unlock": unlock,
            "retry_after_unlock": detail,
            "contacts_after_unlock": _dry_run_contacts_after_unlock(
                client, entity_id, contact_source
            ),
            "final_detail_after_contacts": _execute_operation(
                client, "company-products", query, dry_run=True
            ),
        }
    if not _products_require_unlock(detail):
        return {
            "unlock_performed": False,
            "view_available": _products_view_available(detail),
            "detail": detail,
            "contacts_after_unlock": {
                "performed": False,
                "reason": "unlock_not_required",
            },
        }
    unlock = _execute_operation(client, "company-unlock", {"entityId": entity_id})
    retried = _execute_operation(client, "company-products", query)
    contacts_after_unlock = _contacts_after_unlock(client, entity_id, contact_source)
    confirmed = _execute_operation(client, "company-products", query)
    view_available = _products_view_available(confirmed)
    return {
        "unlock_performed": True,
        "unlock_effective": view_available,
        "view_available": view_available,
        "unlock": unlock,
        "retry_after_unlock": retried,
        "detail": confirmed,
        "contacts_after_unlock": contacts_after_unlock,
    }


def _execute_api_request(
    args: argparse.Namespace,
    client: LixiaoClient,
    query: dict[str, Any],
    body: dict[str, Any] | None,
) -> Any:
    if args.operation == "company-products":
        return _run_company_products(
            client,
            query,
            dry_run=args.dry_run,
            unlock_if_needed=bool(getattr(args, "unlock_if_needed", False)),
            contact_source=getattr(args, "contact_source", None),
        )
    return _execute_operation(
        client, args.operation, query, body=body, dry_run=args.dry_run
    )


def _run_api(args: argparse.Namespace, store: CredentialStore, client: LixiaoClient) -> Any:
    if args.operation == "list":
        return {"count": len(SPECS), "operations": operation_catalog()}
    query = parse_query_pairs(args.query)
    spec = SPECS[args.operation]
    for key in spec.required_query:
        value = getattr(args, f"required_{key}")
        if value:
            query[key] = value
    body = parse_json_input(args.body) if args.body is not None else None
    try:
        return _execute_api_request(args, client, query, body)
    except AuthenticationError:
        if args.dry_run or spec.auth != "app":
            raise
        phone, password = _saved_account(store)
        if not phone or not password:
            raise
        _auto_login(
            client,
            store,
            phone=phone,
            password=password,
            ttocr_url=resolve_ttocr_url(None),
        )
        return _execute_api_request(args, client, query, body)


def _candidate_key(item: dict[str, Any]) -> tuple[str, str, str]:
    return tuple(str(item.get(key) or "") for key in ("id", "uncid", "name"))


def _search_one_platform(
    args: argparse.Namespace,
    client: LixiaoClient,
    config: dict[str, Any] | None,
    requested: str,
) -> tuple[str, dict[str, Any]]:
    selection = resolve_platform(config, requested)
    if args.dry_run:
        body = build_search_body(
            selection, args.keyword, page=1, page_size=args.page_size
        )
        planned = _execute_operation(
            client, "scene-search-products", body=body, dry_run=True
        )
        return selection.label, {"count": 0, "planned_request": planned}
    candidates: list[dict[str, Any]] = []
    seen: set[tuple[str, str, str]] = set()
    total = None
    page_count = (args.limit_per_platform + args.page_size - 1) // args.page_size
    for page in range(1, page_count + 1):
        body = build_search_body(
            selection, args.keyword, page=page, page_size=args.page_size
        )
        response = _execute_operation(client, "scene-search-products", body=body)
        items, total = response_items(response)
        for item in items:
            key = _candidate_key(item)
            if key not in seen and len(candidates) < args.limit_per_platform:
                candidates.append(item)
                seen.add(key)
        if len(items) < args.page_size or len(candidates) >= args.limit_per_platform:
            break
    return selection.label, {
        "count": len(candidates),
        "reported_total": total,
        "candidates": candidates,
    }


def _run_ecommerce_search(args: argparse.Namespace, client: LixiaoClient) -> Any:
    if args.limit_per_platform <= 0 or not 1 <= args.page_size <= 100:
        raise CommandError("limits must be positive and page size must be at most 100")
    config = None
    if not args.dry_run:
        config = _execute_operation(client, "search-condition-config")
    platforms: dict[str, Any] = {}
    for requested in dict.fromkeys(args.platform):
        label, result = _search_one_platform(args, client, config, requested)
        platforms[label] = result
    return {
        "keyword": args.keyword,
        "limit_per_platform": args.limit_per_platform,
        "platforms": platforms,
    }


def _profile_contacts(
    client: LixiaoClient,
    entity_id: str,
    company_name: str,
    contact_source: str | None,
    *,
    dry_run: bool,
) -> tuple[Any, Any]:
    count = _execute_operation(
        client, "company-contacts-count", {"pid": entity_id}, dry_run=dry_run
    )
    query: dict[str, Any] = {"pid": entity_id, "entName": company_name}
    if contact_source:
        query["source"] = contact_source
    contacts = _execute_operation(
        client, "company-contacts", query, dry_run=dry_run
    )
    return count, contacts


def _collect_company_profile(
    args: argparse.Namespace, client: LixiaoClient, entity_id: str
) -> dict[str, Any]:
    card = _execute_operation(
        client, "company-card", {"id": entity_id}, dry_run=args.dry_run
    )
    company_name = _company_name(card) or "resolved from company-card response"
    products = _run_company_products(
        client,
        {"id": entity_id},
        dry_run=args.dry_run,
        unlock_if_needed=True,
        contact_source=args.contact_source,
    )
    operations = {
        "base": "company-base",
        "exhibitions": "company-exhibitions",
        "management": "company-management",
        "recruitment": "company-recruitment",
        "intellectual_property": "company-ip",
        "brand": "company-brand",
    }
    details = {
        key: _execute_operation(
            client, operation, {"id": entity_id}, dry_run=args.dry_run
        )
        for key, operation in operations.items()
    }
    count, contacts = _profile_contacts(
        client,
        entity_id,
        company_name,
        args.contact_source,
        dry_run=args.dry_run,
    )
    return {
        "id": entity_id,
        "company_name": company_name,
        "card": card,
        "products": products,
        **details,
        "contact_count": count,
        "contacts": contacts,
    }


def _run_company_profiles(args: argparse.Namespace, client: LixiaoClient) -> Any:
    ids = list(dict.fromkeys(str(item).strip() for item in args.id if str(item).strip()))
    if not ids:
        raise CommandError("at least one company id is required")
    return {
        "profiles": [_collect_company_profile(args, client, item) for item in ids]
    }


def _workflow_condition(args: argparse.Namespace) -> dict[str, Any]:
    value = getattr(args, "condition", None)
    return empty_condition() if value is None else parse_json_input(value)


def _advanced_candidate_key(item: dict[str, Any]) -> tuple[str, str, str, str]:
    return tuple(str(item.get(key) or "") for key in ("id", "uncid", "name", "title"))


def _run_paginated_search(
    args: argparse.Namespace,
    client: LixiaoClient,
    operation: str,
    body_builder: Callable[[dict[str, Any], int, int], dict[str, Any]],
) -> dict[str, Any]:
    validate_advanced_limits(args.limit, args.page_size)
    condition = _workflow_condition(args)
    if args.dry_run:
        body = body_builder(condition, 1, args.page_size)
        planned = _execute_operation(
            client, operation, body=body, dry_run=True
        )
        return {
            "operation": operation,
            "limit": args.limit,
            "count": 0,
            "planned_request": planned,
        }
    candidates: list[dict[str, Any]] = []
    seen: set[tuple[str, str, str, str]] = set()
    reported_total = None
    page = 1
    while True:
        response = _execute_operation(
            client, operation, body=body_builder(condition, page, args.page_size)
        )
        items, reported_total = advanced_response_items(response)
        for item in items:
            key = _advanced_candidate_key(item)
            if key not in seen and len(candidates) < args.limit:
                seen.add(key)
                candidates.append(item)
        if len(items) < args.page_size or len(candidates) >= args.limit:
            break
        if reported_total is None:
            raise CommandError("Lixiao search response has no total for pagination")
        if page * args.page_size >= reported_total:
            break
        page += 1
    return {
        "operation": operation,
        "limit": args.limit,
        "count": len(candidates),
        "reported_total": reported_total,
        "candidates": candidates,
    }


def _run_advanced_search(args: argparse.Namespace, client: LixiaoClient) -> dict[str, Any]:
    return _run_paginated_search(
        args,
        client,
        "advanced-search",
        lambda condition, page, page_size: build_advanced_search_body(
            condition, page=page, page_size=page_size
        ),
    )


def _run_tender_search(args: argparse.Namespace, client: LixiaoClient) -> dict[str, Any]:
    return _run_paginated_search(
        args,
        client,
        "tender-project-search",
        lambda condition, page, page_size: build_tender_search_body(
            condition, args.keyword, page=page, page_size=page_size
        ),
    )


def _run_channel_search(args: argparse.Namespace, client: LixiaoClient) -> dict[str, Any]:
    return _run_paginated_search(
        args,
        client,
        "channel-search",
        lambda condition, page, page_size: build_channel_search_body(
            condition, args.keyword, page=page, page_size=page_size
        ),
    )


def _run_search_conditions(args: argparse.Namespace, client: LixiaoClient) -> Any:
    query: dict[str, Any] = {
        "groupName": args.group_name,
        "category": args.category,
    }
    if args.module:
        query["moduleName"] = json.dumps(
            list(dict.fromkeys(args.module)), separators=(",", ":")
        )
    return _execute_operation(
        client, "advanced-search-conditions", query, dry_run=args.dry_run
    )


def _run_search_templates(args: argparse.Namespace, client: LixiaoClient) -> Any:
    if args.page_size <= 0 or args.page_num <= 0:
        raise CommandError("template page size and page number must be positive")
    query = {
        "type": str(args.template_type),
        "pageSize": str(args.page_size),
        "pageNum": str(args.page_num),
        "searchName": args.search_name,
    }
    return _execute_operation(client, "search-templates", query, dry_run=args.dry_run)


def _run_workflow(args: argparse.Namespace, client: LixiaoClient) -> Any:
    if args.workflow_action == "ecommerce-search":
        return _run_ecommerce_search(args, client)
    if args.workflow_action == "company-profile":
        return _run_company_profiles(args, client)
    if args.workflow_action == "advanced-search":
        return _run_advanced_search(args, client)
    if args.workflow_action == "tender-search":
        return _run_tender_search(args, client)
    if args.workflow_action == "channel-search":
        return _run_channel_search(args, client)
    if args.workflow_action == "search-conditions":
        return _run_search_conditions(args, client)
    if args.workflow_action == "search-templates":
        return _run_search_templates(args, client)
    raise CommandError(f"unsupported workflow action: {args.workflow_action}")


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


def _run_password(
    args: argparse.Namespace, store: CredentialStore, client: LixiaoClient
) -> Any:
    password = _read_secret("Password")
    body = {
        "challenge": args.challenge,
        "validate": args.validate,
        "seccode": args.seccode,
        "password": password,
        "type": "login",
        "phone": args.phone,
        "agreementVersion": args.agreement_version,
    }
    login = _execute_operation(
        client, "password-login", body=body, dry_run=args.dry_run
    )
    if args.dry_run:
        return login
    result = _finish_password_login(client, login)
    store.update(phone=args.phone, password=password)
    return result


def _run_ensure(args: argparse.Namespace, store: CredentialStore, client: LixiaoClient) -> Any:
    if args.dry_run:
        return _execute_operation(client, "app-session", dry_run=True)
    try:
        return {"status": "valid", "app": _execute_operation(client, "app-session")}
    except LixiaoError as exc:
        if exc.retryable:
            raise
    phone, password = _saved_account(store)
    if not phone or not password:
        raise AuthenticationError("session is invalid and no account is saved")
    result = _auto_login(
        client,
        store,
        phone=phone,
        password=password,
        ttocr_url=resolve_ttocr_url(getattr(args, "ttocr_url", None)),
    )
    return {"status": "reauthenticated", **result}


def _run_auth(
    args: argparse.Namespace, store: CredentialStore, client: LixiaoClient
) -> Any:
    action = args.auth_action
    if action == "status":
        return store.summary()
    if action == "login":
        if args.dry_run:
            return {
                "steps": [
                    "captcha-register",
                    "ttocr-recognize",
                    "password-login",
                    "crm-sso-callback",
                    "crm-pioneers",
                    "app-session",
                ],
                "captcha": _execute_operation(client, "captcha-register", dry_run=True),
            }
        phone, password = _resolve_account(args, store)
        return _auto_login(
            client,
            store,
            phone=phone,
            password=password,
            ttocr_url=resolve_ttocr_url(args.ttocr_url),
        )
    if action == "ensure":
        return _run_ensure(args, store, client)
    if action in {"set-app-token", "set-business-token"}:
        field = "app_token" if action == "set-app-token" else "business_token"
        store.update(**{field: _read_secret(field.replace("_", " ").title())})
        return store.summary()
    if action in {"set-ttocr-token", "set-session-token"}:
        store.update(ttocr_token=_read_secret("Legacy IYW Gateway Token"))
        saved = store.load()
        client.ttocr_token = saved.get("ttocr_token")
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
        return _run_password(args, store, client)
    if action == "app":
        return _execute_operation(client, "app-session", dry_run=args.dry_run)
    raise CommandError(f"unsupported auth action: {action}")


def run(args: argparse.Namespace) -> Any:
    store = CredentialStore(resolve_config_dir(args.config_dir))
    client = LixiaoClient(
        store, timeout=args.timeout, load_credentials=not args.dry_run
    )
    if args.command == "auth":
        return _run_auth(args, store, client)
    if args.command == "workflow":
        return _run_workflow(args, client)
    return _run_api(args, store, client)


def main() -> int:
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="backslashreplace")
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
