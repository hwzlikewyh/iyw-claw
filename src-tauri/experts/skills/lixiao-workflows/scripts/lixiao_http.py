from __future__ import annotations

SERVICE_URLS = {
    "uc": "https://uc.weiwenjia.com",
    "skb": "https://skb.weiwenjia.com",
    "enterprise": "https://enterprise.weiwenjia.com",
}


def base_headers() -> dict[str, str]:
    return {
        "accept": "application/json, text/plain, */*",
        "content-type": "application/json;charset=UTF-8",
        "user-agent": "Mozilla/5.0 lixiao-cli/1.0",
    }


def app_headers(token: str) -> dict[str, str]:
    headers = base_headers()
    headers.update(
        {
            "apptoken": token,
            "platform": "IK",
            "origin": "https://uc.weiwenjia.com",
            "referer": "https://uc.weiwenjia.com/",
        }
    )
    return headers
