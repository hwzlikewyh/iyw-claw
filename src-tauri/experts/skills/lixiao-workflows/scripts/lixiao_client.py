from __future__ import annotations

import json
import os
from http.cookiejar import Cookie, CookieJar
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import parse_qs, urlencode, urlsplit
from urllib.request import HTTPCookieProcessor, Request, build_opener

from lixiao_commands import ApiCall
from lixiao_config import (
    CredentialStore,
    load_iyw_account_access_token,
    public_data,
)
from lixiao_http import SERVICE_URLS, app_headers, base_headers


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


class CredentialRejectedError(AuthenticationError):
    pass


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
        ttocr_token: str | None = None,
        load_credentials: bool = True,
    ):
        self.store = store
        self.timeout = timeout
        saved = store.load() if load_credentials else {}
        self.app_token = (
            app_token
            or (os.getenv("LIXIAO_APP_TOKEN") if load_credentials else None)
            or saved.get("app_token")
        )
        self.business_token = (
            business_token
            or (os.getenv("LIXIAO_BUSINESS_TOKEN") if load_credentials else None)
            or saved.get("business_token")
        )
        self.ttocr_token = (
            (
                load_iyw_account_access_token()
                or ttocr_token
                or os.getenv("IYW_TOKEN")
                or os.getenv("LIXIAO_TTOCR_TOKEN")
                or saved.get("ttocr_token")
                or saved.get("login_token")
            )
            if load_credentials
            else ttocr_token
        )
        self.cookies = CookieJar()
        for item in saved.get("cookies") or []:
            if isinstance(item, dict) and item.get("name"):
                self.cookies.set_cookie(_cookie_from_data(item))
        self.opener = build_opener(HTTPCookieProcessor(self.cookies))

    def execute(self, call: ApiCall, *, dry_run: bool = False) -> Any:
        url = self._url(call)
        if dry_run:
            return public_data(
                {
                    "operation": call.operation,
                    "method": call.endpoint.method,
                    "url": url,
                    "headers": self._dry_run_headers(call.endpoint.auth),
                    "body": call.body,
                }
            )
        headers = self._headers(call.endpoint.auth)
        data = None
        if call.body is not None:
            data = json.dumps(call.body, ensure_ascii=False).encode("utf-8")
        request = Request(url, data=data, method=call.endpoint.method, headers=headers)
        result = self._open(request, call.endpoint.service, operation=call.operation)
        self._save_session(call.operation, result)
        return result

    def _dry_run_headers(self, auth: str) -> dict[str, str]:
        headers = base_headers()
        if auth == "app":
            headers["apptoken"] = "<redacted>"
            return headers
        headers.update(
            {
                "app_token": "<redacted>",
                "authorization": "<redacted>",
                "brand": "%E5%8A%B1%E9%94%80",
                "crm_platform_type": "lixiaoyun",
                "platform_type": "PC",
                "project_name": "%E7%8B%AC%E7%AB%8B",
                "origin": "https://lxcrm.weiwenjia.com",
                "referer": "https://lxcrm.weiwenjia.com/",
            }
        )
        return headers

    def ttocr_headers(self) -> dict[str, str]:
        if not self.ttocr_token:
            raise AuthenticationError(
                "IYW account token is unavailable; sign in to IYW Claw "
                "or configure IYW_TOKEN"
            )
        return {"token": str(self.ttocr_token)}

    def save_business_session(self, token: str) -> None:
        if not token:
            raise AuthenticationError("CRM business token is empty")
        self.business_token = str(token)
        self.store.update(
            app_token=self.app_token,
            business_token=self.business_token,
            cookies=[_cookie_to_data(cookie) for cookie in self.cookies],
        )

    def _url(self, call: ApiCall) -> str:
        base = SERVICE_URLS[call.endpoint.service]
        query = urlencode(call.query, doseq=True)
        return f"{base}{call.endpoint.path}{'?' + query if query else ''}"

    def _headers(self, auth: str) -> dict[str, str]:
        if not self.app_token:
            self._bootstrap_app_token()
        if auth == "app":
            return app_headers(str(self.app_token))
        if not self.business_token:
            raise AuthenticationError(
                "business token is not configured; run auth set-business-token"
            )
        headers = base_headers()
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

    def _bootstrap_app_token(self) -> None:
        bootstrap_token = self._login_entry_token()
        request = Request(
            f"{SERVICE_URLS['uc']}/api/sso/getApp",
            headers=app_headers(bootstrap_token),
        )
        result = self._open(request, "uc", operation="app-token-bootstrap")
        data = result.get("data") if isinstance(result, dict) else None
        token = data.get("appToken") if isinstance(data, dict) else None
        if not token:
            raise AuthenticationError("Lixiao app token was not returned by getApp")
        self.app_token = str(token)
        self.store.update(app_token=self.app_token)

    def _login_entry_token(self) -> str:
        request = Request(
            f"{SERVICE_URLS['uc']}/",
            headers={"user-agent": "Mozilla/5.0 lixiao-cli/1.0"},
        )
        try:
            with self.opener.open(request, timeout=self.timeout) as response:
                final_url = response.geturl()
        except HTTPError as exc:
            exc.read()
            raise AuthenticationError("unable to obtain Lixiao app token") from exc
        except (URLError, TimeoutError) as exc:
            raise LixiaoError(
                f"app token bootstrap failed: {exc}",
                code="upstream_unavailable",
                retryable=True,
            ) from exc

        parsed = urlsplit(final_url)
        token = parse_qs(parsed.query).get("appToken", [""])[0]
        if parsed.scheme != "https" or parsed.netloc != "uc.weiwenjia.com" or not token:
            raise AuthenticationError("Lixiao app bootstrap value was not returned")
        return token

    def _open(self, request: Request, service: str, *, operation: str) -> Any:
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
        self._validate_response(result, status, service=service, operation=operation)
        return result

    def _validate_response(
        self, result: Any, status: int, *, service: str, operation: str
    ) -> None:
        if not 200 <= status < 300:
            message = result.get("message") if isinstance(result, dict) else None
            self._raise_http_error(status, message)
        if not isinstance(result, dict):
            return
        if service == "uc":
            self._validate_uc_result(result, operation=operation)
        self._validate_business_result(result)

    def _validate_business_result(self, result: dict[str, Any]) -> None:
        if result.get("success") is False:
            raise LixiaoError(
                str(result.get("message") or "business request failed"),
                code=str(result.get("error_code") or "request_failed"),
            )

    def _validate_uc_result(self, result: dict[str, Any], *, operation: str) -> None:
        if "code" in result and str(result["code"]) != "0":
            message = str(result.get("message") or "login request failed")
            code = str(result.get("code"))
            if code == "401":
                if operation == "password-login":
                    raise CredentialRejectedError(message)
                raise AuthenticationError(message)
            raise LixiaoError(message, code=code)

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
        if operation == "app-session" and isinstance(data, dict):
            if data.get("appToken"):
                self.app_token = str(data["appToken"])
                values["app_token"] = self.app_token
            if data.get("accessToken"):
                values["access_token"] = data["accessToken"]
        self.store.update(**values)
