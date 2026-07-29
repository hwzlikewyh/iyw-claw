from __future__ import annotations

from html.parser import HTMLParser
from typing import Any
from urllib.parse import urlsplit


class VerificationTokenMissingError(ValueError):
    pass


class _VerificationTokenParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.token = ""

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag.lower() != "input":
            return
        values = {key.lower(): value or "" for key, value in attrs}
        if values.get("name") == "__RequestVerificationToken":
            self.token = values.get("value", "")


class _LoginMessageParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.in_message = False
        self.parts: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        values = {key.lower(): value or "" for key, value in attrs}
        if values.get("id", "").lower() == "msgtip":
            self.in_message = True

    def handle_endtag(self, tag: str) -> None:
        if self.in_message:
            self.in_message = False

    def handle_data(self, data: str) -> None:
        if self.in_message:
            self.parts.append(data)


def extract_verification_token(html: str) -> str:
    parser = _VerificationTokenParser()
    parser.feed(html)
    if not parser.token:
        raise VerificationTokenMissingError(
            "CRM login page did not contain a verification token"
        )
    return parser.token


def extract_login_message(html: str) -> str:
    parser = _LoginMessageParser()
    parser.feed(html)
    return " ".join("".join(parser.parts).split())[:160]


def is_login_page(response: Any) -> bool:
    path = urlsplit(response.url).path.rstrip("/").lower()
    if path == "/home/login":
        return True
    text = response.text.lower()
    return 'name="password"' in text and "__requestverificationtoken" in text
