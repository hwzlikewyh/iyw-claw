from __future__ import annotations

import sys
from pathlib import Path
from unittest.mock import Mock

import pytest

SCRIPTS_DIR = Path(__file__).parents[1] / "lixiao-workflows" / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))

import lixiao
from lixiao_client import (
    AuthenticationError,
    CredentialRejectedError,
    LixiaoClient,
    LixiaoError,
)
from lixiao_config import CredentialStore


def test_password_login_uc_401_is_credential_rejection(tmp_path):
    client = LixiaoClient(CredentialStore(tmp_path), load_credentials=False)

    with pytest.raises(CredentialRejectedError, match="rejected"):
        client._validate_response(
            {"success": False, "code": 401, "message": "rejected"},
            200,
            service="uc",
            operation="password-login",
        )


def test_nonpassword_uc_401_is_authentication_error(tmp_path):
    client = LixiaoClient(CredentialStore(tmp_path), load_credentials=False)

    with pytest.raises(AuthenticationError, match="expired") as error:
        client._validate_uc_result(
            {"code": 401, "message": "expired"}, operation="app-session"
        )

    assert type(error.value) is AuthenticationError


def test_password_login_http_401_is_not_credential_rejection(tmp_path):
    client = LixiaoClient(CredentialStore(tmp_path), load_credentials=False)

    with pytest.raises(AuthenticationError, match="HTTP unauthorized") as error:
        client._raise_http_error(401, "HTTP unauthorized")

    assert type(error.value) is AuthenticationError


def test_password_login_extended_uc_code_is_not_credential_rejection(tmp_path):
    client = LixiaoClient(CredentialStore(tmp_path), load_credentials=False)

    with pytest.raises(LixiaoError, match="other failure") as error:
        client._validate_response(
            {"success": False, "code": 1401, "message": "other failure"},
            200,
            service="uc",
            operation="password-login",
        )

    assert type(error.value) is LixiaoError


def test_auto_login_saves_credentials_before_downstream_session(monkeypatch, tmp_path):
    store = CredentialStore(tmp_path)
    client = Mock(timeout=30)
    client.ttocr_headers.return_value = {"token": "redacted"}
    execute = Mock(
        side_effect=[
            {"data": {"gt": "captcha-gt", "challenge": "captcha-challenge"}},
            {"code": 0, "data": {"ticket": "ticket"}},
        ]
    )
    monkeypatch.setattr(lixiao, "_execute_operation", execute)
    monkeypatch.setattr(
        lixiao,
        "solve_geetest",
        Mock(
            return_value={
                "challenge": "solved-challenge",
                "validate": "validate",
                "seccode": "seccode",
            }
        ),
    )
    monkeypatch.setattr(
        lixiao,
        "_finish_password_login",
        Mock(side_effect=AuthenticationError("SSO unavailable")),
    )

    with pytest.raises(AuthenticationError, match="SSO unavailable"):
        lixiao._auto_login(
            client,
            store,
            phone="new-phone",
            password="new-pass",
        )

    assert store.saved_credentials() == ("new-phone", "new-pass")


def test_manual_password_saves_credentials_before_downstream_session(
    monkeypatch, tmp_path
):
    store = CredentialStore(tmp_path)
    args = lixiao.build_parser().parse_args(
        [
            "auth",
            "password",
            "--phone",
            "new-phone",
            "--challenge",
            "challenge",
            "--validate",
            "validate",
            "--seccode",
            "seccode",
        ]
    )
    monkeypatch.setattr(lixiao, "_read_secret", Mock(return_value="new-pass"))
    monkeypatch.setattr(
        lixiao,
        "_execute_operation",
        Mock(return_value={"code": 0, "data": {"ticket": "ticket"}}),
    )
    monkeypatch.setattr(
        lixiao,
        "_finish_password_login",
        Mock(side_effect=AuthenticationError("SSO unavailable")),
    )

    with pytest.raises(AuthenticationError, match="SSO unavailable"):
        lixiao._run_password(args, store, Mock())

    assert store.saved_credentials() == ("new-phone", "new-pass")
