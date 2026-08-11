from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock

import pytest


SCRIPTS_DIR = Path(__file__).parents[1] / "iyw-crm-workflows" / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))

import iyw_crm  # noqa: E402
from iyw_crm_client import (  # noqa: E402
    AuthenticationError,
    CredentialRejectedError,
    CrmClient,
    CrmError,
    HttpResponse,
)
from iyw_crm_config import SessionStore  # noqa: E402


def _saved_store(tmp_path: Path) -> SessionStore:
    store = SessionStore(tmp_path)
    store.update(username="saved-user", password="saved-pass", cookies=[])
    return store


def test_session_store_reports_and_invalidates_saved_credentials(tmp_path):
    store = _saved_store(tmp_path)

    assert store.saved_credentials() == ("saved-user", "saved-pass")
    assert store.summary()["has_saved_account"] is True
    assert store.summary()["has_saved_credentials"] is True

    assert store.invalidate_saved_credentials() is True
    assert store.load() == {"version": 1, "username": "saved-user"}
    assert store.summary()["has_saved_account"] is True
    assert store.summary()["has_saved_credentials"] is False


def test_status_reads_saved_credentials_in_new_process(tmp_path):
    _saved_store(tmp_path)

    completed = subprocess.run(
        [
            sys.executable,
            str(SCRIPTS_DIR / "iyw_crm.py"),
            "--config-dir",
            str(tmp_path),
            "auth",
            "status",
        ],
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    data = json.loads(completed.stdout)["data"]

    assert data["has_saved_account"] is True
    assert data["has_saved_credentials"] is True
    assert "saved-user" not in completed.stdout
    assert "saved-pass" not in completed.stdout


def test_password_only_login_reuses_saved_username(tmp_path):
    store = _saved_store(tmp_path)
    args = iyw_crm.build_parser().parse_args(
        ["auth", "login", "--password", "replacement-pass"]
    )

    assert iyw_crm._resolve_login_credentials(args, store) == (
        "saved-user",
        "replacement-pass",
    )


def test_password_only_login_requires_saved_username(tmp_path):
    store = SessionStore(tmp_path)
    args = iyw_crm.build_parser().parse_args(
        ["auth", "login", "--password", "replacement-pass"]
    )

    with pytest.raises(ValueError, match="saved CRM username"):
        iyw_crm._resolve_login_credentials(args, store)


def test_successful_client_login_persists_password(tmp_path):
    store = SessionStore(tmp_path)
    client = CrmClient(store)
    login_page = HttpResponse(
        status=200,
        content_type="text/html",
        text='<input name="__RequestVerificationToken" value="form-token">',
        url="http://crm.chdesign.com.cn/Home/Login",
    )
    authenticated_page = HttpResponse(
        status=200,
        content_type="text/html",
        text="<html>authenticated</html>",
        url="http://crm.chdesign.com.cn/",
    )
    client._request = Mock(
        side_effect=[login_page, authenticated_page, authenticated_page]
    )

    result = client.login("saved-user", "saved-pass")

    assert result["authenticated"] is True
    assert store.saved_credentials() == ("saved-user", "saved-pass")


def test_saved_login_credential_rejection_invalidates_password_and_session(tmp_path):
    store = _saved_store(tmp_path)
    client = Mock()
    client.login.side_effect = CredentialRejectedError("rejected")

    with pytest.raises(CredentialRejectedError, match="rejected"):
        iyw_crm._login_with_saved_credentials(client, store)

    client.login.assert_called_once_with("saved-user", "saved-pass")
    assert store.load() == {"version": 1, "username": "saved-user"}


def test_saved_login_noncredential_auth_failure_preserves_password(tmp_path):
    store = _saved_store(tmp_path)
    client = Mock()
    client.login.side_effect = AuthenticationError("verification token missing")

    with pytest.raises(AuthenticationError, match="verification token missing"):
        iyw_crm._login_with_saved_credentials(client, store)

    assert store.saved_credentials() == ("saved-user", "saved-pass")


def test_saved_login_network_failure_preserves_password(tmp_path):
    store = _saved_store(tmp_path)
    client = Mock()
    client.login.side_effect = CrmError(
        "offline", code="upstream_unavailable", retryable=True
    )

    with pytest.raises(CrmError, match="offline"):
        iyw_crm._login_with_saved_credentials(client, store)

    assert store.saved_credentials() == ("saved-user", "saved-pass")


def test_client_classifies_explicit_credential_rejection(tmp_path):
    store = SessionStore(tmp_path)
    client = CrmClient(store)
    login_page = HttpResponse(
        status=200,
        content_type="text/html",
        text='<input name="__RequestVerificationToken" value="form-token">',
        url="http://crm.chdesign.com.cn/Home/Login",
    )
    rejected_page = HttpResponse(
        status=200,
        content_type="text/html",
        text='<div id="msgTip">用户名或密码错误</div>',
        url="http://crm.chdesign.com.cn/Home/Login",
    )
    client._request = Mock(side_effect=[login_page, rejected_page])

    with pytest.raises(CredentialRejectedError, match="用户名或密码错误"):
        client.login("saved-user", "saved-pass")


def test_client_preserves_unknown_login_page_failure(tmp_path):
    store = SessionStore(tmp_path)
    client = CrmClient(store)
    login_page = HttpResponse(
        status=200,
        content_type="text/html",
        text='<input name="__RequestVerificationToken" value="form-token">',
        url="http://crm.chdesign.com.cn/Home/Login",
    )
    unknown_failure = HttpResponse(
        status=200,
        content_type="text/html",
        text='<div id="msgTip">登录服务暂不可用</div>',
        url="http://crm.chdesign.com.cn/Home/Login",
    )
    client._request = Mock(side_effect=[login_page, unknown_failure])

    with pytest.raises(AuthenticationError, match="登录服务暂不可用") as error:
        client.login("saved-user", "saved-pass")

    assert type(error.value) is AuthenticationError


def test_customer_search_reauthenticates_and_replays_once(monkeypatch, tmp_path):
    store = _saved_store(tmp_path)
    client = Mock()
    client.search_customers.side_effect = [
        AuthenticationError("expired"),
        {"total": 1, "rows": [{"id": 1}]},
    ]
    client.login.return_value = {"authenticated": True}
    monkeypatch.setattr(iyw_crm, "_client", lambda args, current: client)
    args = SimpleNamespace(
        operation="customer-search",
        text="test",
        page=1,
        rows=15,
        field=[],
        dry_run=False,
    )

    result = iyw_crm._run_api(args, store)

    assert result == {"total": 1, "rows": [{"id": 1}]}
    assert client.search_customers.call_count == 2
    client.login.assert_called_once_with("saved-user", "saved-pass")


def test_auth_ensure_reauthenticates_once(monkeypatch, tmp_path):
    store = _saved_store(tmp_path)
    client = Mock()
    client.ensure_authenticated.side_effect = AuthenticationError("expired")
    client.login.return_value = {"authenticated": True, "session_saved": True}
    monkeypatch.setattr(iyw_crm, "_client", lambda args, current: client)
    args = SimpleNamespace(
        auth_action="ensure",
        dry_run=False,
        base_url="http://crm.chdesign.com.cn",
    )

    result = iyw_crm._run_auth(args, store)

    assert result["status"] == "reauthenticated"
    client.ensure_authenticated.assert_called_once_with()
    client.login.assert_called_once_with("saved-user", "saved-pass")


def test_auth_status_dry_run_does_not_read_store():
    store = Mock()
    store.summary.side_effect = AssertionError("credentials were read")
    args = SimpleNamespace(
        auth_action="status",
        dry_run=True,
        base_url="http://crm.chdesign.com.cn",
    )

    result = iyw_crm._run_auth(args, store)

    assert result["operation"] == "auth-status"
    assert result["credentials_read"] is False
    store.summary.assert_not_called()
