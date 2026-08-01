from __future__ import annotations

import re
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import Request

from lixiao_client import AuthenticationError


CRM_BASE_URL = "https://lxcrm.weiwenjia.com"
BUSINESS_TOKEN_PATTERN = re.compile(
    r"\bwindow\.current_user_token\s*=\s*(['\"])(?P<token>[^'\"]+)\1"
)


class CrmSessionError(AuthenticationError):
    code = "crm_session_failed"


def bootstrap_crm_session(client: Any, login_result: Any) -> dict[str, Any]:
    ticket, gid = _callback_values(login_result)
    if not client.app_token:
        raise CrmSessionError("app token is required for CRM SSO callback")
    query = urlencode(
        {
            "st": ticket,
            "platform": "IK",
            "appToken": client.app_token,
            "state": "",
            "x-lx-gid": gid,
        }
    )
    _read_html(client, f"{CRM_BASE_URL}?{query}", stage="CRM SSO callback")
    html = _read_html(client, f"{CRM_BASE_URL}/pioneers", stage="CRM pioneers")
    client.save_business_session(_extract_business_token(html))
    return {"status": "authenticated", "business_token_saved": True}


def _callback_values(login_result: Any) -> tuple[str, str]:
    data = login_result.get("data") if isinstance(login_result, dict) else None
    ticket = data.get("ticket") if isinstance(data, dict) else None
    gid = data.get("x-lx-gid") if isinstance(data, dict) else None
    if not ticket or not gid:
        raise CrmSessionError("password login did not return ticket and x-lx-gid")
    return str(ticket), str(gid)


def _read_html(client: Any, url: str, *, stage: str) -> str:
    request = Request(
        url,
        method="GET",
        headers={
            "accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            "referer": "https://uc.weiwenjia.com/",
            "user-agent": "Mozilla/5.0 lixiao-cli/1.0",
        },
    )
    try:
        with client.opener.open(request, timeout=client.timeout) as response:
            raw = response.read()
    except HTTPError as exc:
        exc.read()
        raise CrmSessionError(f"{stage} failed with HTTP {exc.code}") from exc
    except (URLError, TimeoutError) as exc:
        raise CrmSessionError(
            f"{stage} unavailable: {exc}", retryable=True
        ) from exc
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise CrmSessionError(f"{stage} returned invalid UTF-8") from exc


def _extract_business_token(html: str) -> str:
    match = BUSINESS_TOKEN_PATTERN.search(html)
    if not match:
        raise CrmSessionError("CRM pioneers page did not expose a business token")
    return match.group("token")
