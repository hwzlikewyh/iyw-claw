from __future__ import annotations

import json
import os
from http.cookiejar import Cookie, CookieJar
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import HTTPCookieProcessor, Request, build_opener

from lixiao_commands import ApiCall
from lixiao_config import CredentialStore, public_data


SERVICE_URLS = {
    "uc": "https://uc.weiwenjia.com",
    "skb": "https://skb.weiwenjia.com",
    "enterprise": "https://enterprise.weiwenjia.com",
}


class LixiaoError(RuntimeError):
    code = "request_failed"
    retryable = False

    def __init__(
        self, message: str, *, code: str | None = None, retryable: bool = False
    ):
        super().__init__(message)
        if code:
            self.code = code
        self.retryable = retryable


class AuthenticationError(LixiaoError):
    code = "authentication_required"


def _cookie_from_data(item: dict[str, Any]) -> Cookie:
    domain = str(item.get("domain") or "")
    path = str(item.get("path") or "/")
    expires = item.get("expires")
    return Cookie(
        version=0,
        name=str(item.get("name") or ""),
        value=str(item.get("value") or ""),
        port=None,
        port_specified=False,
        domain=domain,
        domain_specified=bool(domain),
        domain_initial_dot=domain.startswith("."),
        path=path,
        path_specified=True,
        secure=bool(item.get("secure")),
        expires=int(expires) if expires is not None else None,
        discard=expires is None,
        comment=None,
        comment_url=None,
        rest=dict(item.get("rest") or {}),
        rfc2109=False,
    )


def _cookie_to_data(cookie: Cookie) -> dict[str, Any]:
    return {
        "name": cookie.name,
        "value": cookie.value,
        "domain": cookie.domain,
        "path": cookie.path,
        "secure": cookie.secure,
        "expires": cookie.expires,
        "rest": dict(cookie._rest),
    }


class LixiaoClient:
    def __init__(
        self,
        store: CredentialStore,
        timeout: float = 30,
        *,
        app_token: str | None = None,
        business_token: str | None = None,
    ):
        self.store = store
        self.timeout = timeout
        saved = store.load()
        self.app_token = (
            app_token or os.getenv("LIXIAO_APP_TOKEN") or saved.get("app_token")
        )
        self.business_token = (
            business_token
            or os.getenv("LIXIAO_BUSINESS_TOKEN")
            or saved.get("business_token")
        )
        self.cookies = CookieJar()
        for item in saved.get("cookies") or []:
            if isinstance(item, dict) and item.get("name"):
                self.cookies.set_cookie(_cookie_from_data(item))
        self.opener = build_opener(HTTPCookieProcessor(self.cookies))

    def execute(self, call: ApiCall, *, dry_run: bool = False) -> Any:
        url = self._url(call)
        headers = self._headers(call.endpoint.auth)
        data = None
        if call.body is not None:
            data = json.dumps(call.body, ensure_ascii=False).encode("utf-8")
        if dry_run:
            return public_data(
                {
                    "operation": call.operation,
                    "method": call.endpoint.method,
                    "url": url,
                    "headers": headers,
                    "body": call.body,
                }
            )
        request = Request(url, data=data, method=call.endpoint.method, headers=headers)
        result = self._open(request, call.endpoint.service)
        self._save_session(call.operation, result)
        return result

    def _url(self, call: ApiCall) -> str:
        base = SERVICE_URLS[call.endpoint.service]
        query = urlencode(call.query, doseq=True)
        return f"{base}{call.endpoint.path}{'?' + query if query else ''}"

    def _headers(self, auth: str) -> dict[str, str]:
        if not self.app_token:
            raise AuthenticationError(
                "app token is not configured; run auth set-app-token"
            )
        headers = {
            "accept": "application/json, text/plain, */*",
            "content-type": "application/json;charset=UTF-8",
            "user-agent": "Mozilla/5.0 lixiao-cli/1.0",
        }
        if auth == "app":
            headers.update(
                {
                    "apptoken": str(self.app_token),
                    "origin": "https://uc.weiwenjia.com",
                    "referer": "https://uc.weiwenjia.com/",
                }
            )
            return headers
        if not self.business_token:
            raise AuthenticationError(
                "business token is not configured; run auth set-business-token"
            )
        headers.update(
            {
                "app_token": str(self.app_token),
                "authorization": f"Token token={self.business_token}",
                "brand": "%E5%8A%B1%E9%94%80",
                "crm_platform_type": "lixiaoyun",
                "platform_type": "PC",
                "project_name": "%E7%8B%AC%E7%AB%8B",
                "origin": "https://lxcrm.weiwenjia.com",
                "referer": "https://lxcrm.weiwenjia.com/",
            }
        )
        return headers

    def _open(self, request: Request, service: str) -> Any:
        try:
            with self.opener.open(request, timeout=self.timeout) as response:
                raw = response.read()
                status = response.status
        except HTTPError as exc:
            raw = exc.read()
            status = exc.code
        except (URLError, TimeoutError) as exc:
            raise LixiaoError(
                f"request failed: {exc}", code="upstream_unavailable", retryable=True
            ) from exc
        try:
            result = json.loads(raw.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            if not 200 <= status < 300:
                self._raise_http_error(status)
            raise LixiaoError(
                "backend returned invalid JSON",
                code="upstream_unavailable",
                retryable=True,
            ) from exc
        self._validate_response(result, status, service=service)
        return result

    def _validate_response(self, result: Any, status: int, *, service: str) -> None:
        if not 200 <= status < 300:
            message = result.get("message") if isinstance(result, dict) else None
            self._raise_http_error(status, message)
        if not isinstance(result, dict):
            return
        self._validate_business_result(result)
        if service == "uc":
            self._validate_uc_result(result)

    def _validate_business_result(self, result: dict[str, Any]) -> None:
        if result.get("success") is False:
            raise LixiaoError(
                str(result.get("message") or "business request failed"),
                code=str(result.get("error_code") or "request_failed"),
            )

    def _validate_uc_result(self, result: dict[str, Any]) -> None:
        if "code" in result and str(result["code"]) != "0":
            raise LixiaoError(
                str(result.get("message") or "login request failed"),
                code=str(result.get("code")),
            )

    def _raise_http_error(self, status: int, message: Any = None) -> None:
        text = str(message or f"HTTP {status}")
        if status == 401:
            raise AuthenticationError(text)
        codes = {403: "permission_denied", 429: "rate_limited"}
        raise LixiaoError(
            text,
            code=codes.get(status, f"http_{status}"),
            retryable=status == 429 or status >= 500,
        )

    def _save_session(self, operation: str, result: Any) -> None:
        values: dict[str, Any] = {
            "app_token": self.app_token,
            "cookies": [_cookie_to_data(cookie) for cookie in self.cookies],
        }
        data = result.get("data") if isinstance(result, dict) else None
        if (
            operation == "app-session"
            and isinstance(data, dict)
            and data.get("accessToken")
        ):
            values["access_token"] = data["accessToken"]
        self.store.update(**values)
