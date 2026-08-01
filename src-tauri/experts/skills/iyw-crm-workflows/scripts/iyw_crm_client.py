from __future__ import annotations

import json
from dataclasses import dataclass
from http.cookiejar import CookieJar
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode, urljoin, urlsplit
from urllib.request import (
    HTTPCookieProcessor,
    HTTPRedirectHandler,
    Request,
    build_opener,
)

from iyw_crm_config import SessionStore, public_data
from iyw_crm_cookies import cookie_from_data, cookie_to_data
from iyw_crm_html import (
    VerificationTokenMissingError,
    extract_login_message,
    extract_verification_token,
)
from iyw_crm_html import (
    is_login_page as _is_login_page,
)

DEFAULT_CRM_BASE_URL = "http://crm.chdesign.com.cn"
FUSION_API_BASE_URL = "https://gateway.iyw.cn/iyw-fusion-api"
USER_AGENT = (
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
    "AppleWebKit/537.36 (KHTML, like Gecko) "
    "Chrome/150.0.0.0 Safari/537.36"
)


class CrmError(RuntimeError):
    code = "request_failed"
    retryable = False

    def __init__(
        self, message: str, *, code: str | None = None, retryable: bool = False
    ):
        super().__init__(message)
        if code:
            self.code = code
        self.retryable = retryable


class AuthenticationError(CrmError):
    code = "authentication_required"


class ConfigurationError(CrmError):
    code = "invalid_configuration"


@dataclass(frozen=True)
class HttpResponse:
    status: int
    content_type: str
    text: str
    url: str


class _SameOriginRedirectHandler(HTTPRedirectHandler):
    def __init__(self, base_url: str):
        super().__init__()
        self.base_url = base_url

    def redirect_request(
        self, req: Any, fp: Any, code: int, msg: str, headers: Any, newurl: str
    ) -> Request | None:
        _ensure_same_origin(self.base_url, newurl)
        return super().redirect_request(req, fp, code, msg, headers, newurl)


class CrmClient:
    def __init__(
        self,
        store: SessionStore,
        timeout: float = 30,
        *,
        base_url: str = DEFAULT_CRM_BASE_URL,
        allow_insecure_http: bool = False,
        load_session: bool = True,
    ):
        self.store = store
        self.timeout = timeout
        self.base_url = _validate_base_url(base_url, allow_insecure_http)
        self.cookies = CookieJar()
        saved = store.load() if load_session else {}
        for item in saved.get("cookies") or []:
            if isinstance(item, dict) and item.get("name"):
                self.cookies.set_cookie(cookie_from_data(item))
        self.opener = build_opener(
            HTTPCookieProcessor(self.cookies),
            _SameOriginRedirectHandler(self.base_url),
        )

    def login(self, username: str, password: str) -> dict[str, Any]:
        self.cookies.clear()
        page = self._request("GET", "/Home/Login")
        try:
            token = extract_verification_token(page.text)
        except VerificationTokenMissingError as exc:
            raise AuthenticationError(str(exc)) from exc
        result = self._request(
            "POST",
            "/Home/Login",
            form={
                "__RequestVerificationToken": token,
                "UserName": username,
                "Password": password,
            },
            headers={
                "origin": self.base_url,
                "referer": self._url("/Home/Login"),
            },
        )
        if _is_login_page(result):
            detail = extract_login_message(result.text)
            path = urlsplit(result.url).path or "/"
            suffix = f": {detail}" if detail else ""
            raise AuthenticationError(
                f"CRM login failed (HTTP {result.status}, final path {path}){suffix}"
            )
        verified = self.ensure_authenticated(persist=False)
        self._save_session(username)
        verified["session_saved"] = True
        return verified

    def ensure_authenticated(self, *, persist: bool = True) -> dict[str, Any]:
        response = self._request("GET", "/")
        if _is_login_page(response):
            raise AuthenticationError("CRM session is missing or expired")
        if persist:
            self._save_session()
        return {"authenticated": True, "session_saved": persist}

    def search_customers(
        self, form: dict[str, str], *, dry_run: bool = False
    ) -> dict[str, Any]:
        headers = {
            "accept": "application/json, text/javascript, */*; q=0.01",
            "referer": self._url("/Customer/Index"),
            "x-requested-with": "XMLHttpRequest",
        }
        if dry_run:
            return public_data(
                {
                    "operation": "customer-search",
                    "method": "POST",
                    "url": self._url("/Customer"),
                    "headers": headers,
                    "form": form,
                }
            )
        response = self._request("POST", "/Customer", form=form, headers=headers)
        if _is_login_page(response):
            raise AuthenticationError("CRM session expired")
        result = _parse_customer_response(response)
        self._save_session()
        return result

    def _save_session(self, username: str | None = None) -> None:
        values: dict[str, Any] = {
            "cookies": [cookie_to_data(item) for item in self.cookies]
        }
        if username:
            values["username"] = username
        self.store.update(**values)

    def _url(self, path: str) -> str:
        return urljoin(self.base_url + "/", path.lstrip("/"))

    def _request(
        self,
        method: str,
        path: str,
        *,
        form: dict[str, str] | None = None,
        headers: dict[str, str] | None = None,
    ) -> HttpResponse:
        data = urlencode(form).encode("utf-8") if form is not None else None
        request_headers = {
            "accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            "accept-language": "zh-CN,zh;q=0.9",
            "cache-control": "max-age=0",
            "upgrade-insecure-requests": "1",
            "user-agent": USER_AGENT,
        }
        request_headers.update(headers or {})
        if form is not None:
            request_headers["content-type"] = (
                "application/x-www-form-urlencoded; charset=UTF-8"
            )
        request = Request(
            self._url(path), data=data, method=method, headers=request_headers
        )
        try:
            with self.opener.open(request, timeout=self.timeout) as response:
                raw = response.read()
                final_url = response.geturl()
                _ensure_same_origin(self.base_url, final_url)
                return _decode_response(
                    response.status, response.headers, raw, final_url
                )
        except HTTPError as exc:
            exc.read()
            if exc.code in {401, 403}:
                raise AuthenticationError(f"CRM returned HTTP {exc.code}") from exc
            raise CrmError(
                f"CRM returned HTTP {exc.code}",
                code=f"http_{exc.code}",
                retryable=exc.code == 429 or exc.code >= 500,
            ) from exc
        except (URLError, TimeoutError) as exc:
            raise CrmError(
                f"CRM request failed: {exc}",
                code="upstream_unavailable",
                retryable=True,
            ) from exc


def _validate_base_url(value: str, allow_insecure_http: bool) -> str:
    parsed = urlsplit(value)
    if parsed.scheme not in {"http", "https"} or not parsed.netloc:
        raise ConfigurationError("CRM base URL must be an HTTP(S) origin")
    if parsed.username or parsed.password or parsed.query or parsed.fragment:
        raise ConfigurationError(
            "CRM base URL must not contain credentials or query data"
        )
    if parsed.path not in {"", "/"}:
        raise ConfigurationError("CRM base URL must be an origin without a path")
    normalized = value.rstrip("/")
    fixed_origin = normalized.casefold() == DEFAULT_CRM_BASE_URL.casefold()
    if parsed.scheme == "http" and not (fixed_origin or allow_insecure_http):
        raise ConfigurationError(
            "custom CRM HTTP origins require --allow-insecure-http"
        )
    return normalized


def _ensure_same_origin(base_url: str, final_url: str) -> None:
    expected = urlsplit(base_url)
    actual = urlsplit(final_url)
    if (expected.scheme.lower(), expected.netloc.lower()) != (
        actual.scheme.lower(),
        actual.netloc.lower(),
    ):
        raise ConfigurationError("CRM redirected to an unexpected origin")


def _decode_response(status: int, headers: Any, raw: bytes, url: str) -> HttpResponse:
    charset = headers.get_content_charset() or "utf-8"
    try:
        text = raw.decode(charset)
    except (LookupError, UnicodeDecodeError) as exc:
        raise CrmError(
            "CRM returned undecodable text", code="invalid_response"
        ) from exc
    return HttpResponse(status, str(headers.get("Content-Type") or ""), text, url)


def _parse_customer_response(response: HttpResponse) -> dict[str, Any]:
    try:
        result = json.loads(response.text)
    except json.JSONDecodeError as exc:
        raise CrmError(
            "CRM customer search returned invalid JSON", code="invalid_response"
        ) from exc
    if not isinstance(result, dict) or not isinstance(result.get("rows"), list):
        raise CrmError(
            "CRM customer search returned an unexpected shape", code="invalid_response"
        )
    if not isinstance(result.get("total"), int):
        raise CrmError("CRM customer search omitted total", code="invalid_response")
    return result
