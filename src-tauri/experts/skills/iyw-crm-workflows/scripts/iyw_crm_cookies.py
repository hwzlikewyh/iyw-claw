from __future__ import annotations

from http.cookiejar import Cookie
from typing import Any


def cookie_from_data(item: dict[str, Any]) -> Cookie:
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


def cookie_to_data(cookie: Cookie) -> dict[str, Any]:
    return {
        "name": cookie.name,
        "value": cookie.value,
        "domain": cookie.domain,
        "path": cookie.path,
        "secure": cookie.secure,
        "expires": cookie.expires,
        "rest": dict(cookie._rest),
    }
