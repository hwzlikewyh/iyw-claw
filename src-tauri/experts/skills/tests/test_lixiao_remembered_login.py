from __future__ import annotations

import json
import subprocess
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
    LixiaoError,
)
from lixiao_config import CredentialStore


def _saved_store(tmp_path: Path) -> CredentialStore:
    store = CredentialStore(tmp_path)
    store.update(
        phone="saved-phone",
        password="saved-pass",
        cookies=[{"name": "session"}],
        app_token="application-token",
        access_token="access-token",
        business_token="business-token",
        ttocr_token="ttocr-token",
        login_token="legacy-ttocr-token",
        refresh_token="refresh-token",
    )
    return store


def test_store_reports_and_invalidates_only_user_credentials(tmp_path, monkeypatch):
    monkeypatch.setattr("lixiao_config.load_iyw_account_access_token", lambda: "")
    store = _saved_store(tmp_path)

    assert store.saved_credentials() == ("saved-phone", "saved-pass")
    assert store.summary()["has_saved_account"] is True
    assert store.summary()["has_saved_credentials"] is True
    assert store.summary()["has_account"] is True

    assert store.invalidate_saved_credentials() is True
    saved = store.load()
    assert saved["phone"] == "saved-phone"
    assert saved["app_token"] == "application-token"
    assert saved["ttocr_token"] == "ttocr-token"
    assert saved["login_token"] == "legacy-ttocr-token"
    for field in (
        "password",
        "cookies",
        "access_token",
        "business_token",
        "refresh_token",
    ):
        assert field not in saved
    assert store.summary()["has_saved_account"] is True
    assert store.summary()["has_saved_credentials"] is False


def test_status_reads_saved_credentials_in_new_process(tmp_path):
    _saved_store(tmp_path)

    completed = subprocess.run(
        [
            sys.executable,
            str(SCRIPTS_DIR / "lixiao.py"),
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
    assert "saved-phone" not in completed.stdout
    assert "saved-pass" not in completed.stdout


def test_password_only_login_reuses_saved_phone(tmp_path):
    store = _saved_store(tmp_path)
    args = lixiao.build_parser().parse_args(
        ["auth", "login", "--password", "replacement-pass"]
    )

    assert lixiao._resolve_account(args, store) == (
        "saved-phone",
        "replacement-pass",
    )


def test_password_only_login_requires_saved_phone(tmp_path):
    store = CredentialStore(tmp_path)
    args = lixiao.build_parser().parse_args(
        ["auth", "login", "--password", "replacement-pass"]
    )

    with pytest.raises(lixiao.CommandError, match="saved Lixiao account"):
        lixiao._resolve_account(args, store)


def test_saved_login_credential_rejection_invalidates_password(monkeypatch, tmp_path):
    store = _saved_store(tmp_path)
    client = Mock()
    login = Mock(side_effect=CredentialRejectedError("rejected"))
    monkeypatch.setattr(lixiao, "_auto_login", login)

    with pytest.raises(CredentialRejectedError, match="rejected"):
        lixiao._login_with_saved_credentials(client, store)

    login.assert_called_once_with(
        client,
        store,
        phone="saved-phone",
        password="saved-pass",
        ttocr_url=None,
    )
    assert store.saved_credentials() == ("saved-phone", None)


def test_saved_login_downstream_auth_failure_preserves_password(monkeypatch, tmp_path):
    store = _saved_store(tmp_path)
    client = Mock()
    monkeypatch.setattr(
        lixiao,
        "_auto_login",
        Mock(side_effect=AuthenticationError("CRM business token is empty")),
    )

    with pytest.raises(AuthenticationError, match="CRM business token is empty"):
        lixiao._login_with_saved_credentials(client, store)

    assert store.saved_credentials() == ("saved-phone", "saved-pass")


def test_saved_login_network_failure_preserves_password(monkeypatch, tmp_path):
    store = _saved_store(tmp_path)
    client = Mock()
    monkeypatch.setattr(
        lixiao,
        "_auto_login",
        Mock(
            side_effect=LixiaoError(
                "offline", code="upstream_unavailable", retryable=True
            )
        ),
    )

    with pytest.raises(LixiaoError, match="offline"):
        lixiao._login_with_saved_credentials(client, store)

    assert store.saved_credentials() == ("saved-phone", "saved-pass")


def test_read_api_reauthenticates_and_replays_once(monkeypatch, tmp_path):
    store = _saved_store(tmp_path)
    client = Mock()
    execute = Mock(side_effect=[AuthenticationError("expired"), {"ok": True}])
    login = Mock(return_value={"status": "reauthenticated"})
    monkeypatch.setattr(lixiao, "_execute_api_request", execute)
    monkeypatch.setattr(lixiao, "_login_with_saved_credentials", login)
    args = lixiao.build_parser().parse_args(["api", "feature-packages"])

    assert lixiao._run_api(args, store, client) == {"ok": True}
    assert execute.call_count == 2
    login.assert_called_once_with(client, store, ttocr_url=None)


def test_side_effect_api_preflights_but_does_not_replay(monkeypatch, tmp_path):
    store = _saved_store(tmp_path)
    client = Mock()
    ensure = Mock(return_value={"status": "valid"})
    execute = Mock(side_effect=AuthenticationError("expired"))
    login = Mock()
    monkeypatch.setattr(lixiao, "_ensure_session", ensure)
    monkeypatch.setattr(lixiao, "_execute_api_request", execute)
    monkeypatch.setattr(lixiao, "_login_with_saved_credentials", login)
    args = lixiao.build_parser().parse_args(
        ["api", "company-unlock", "--entity-id", "company-id"]
    )

    with pytest.raises(AuthenticationError, match="expired"):
        lixiao._run_api(args, store, client)

    ensure.assert_called_once_with(client, store, ttocr_url=None)
    execute.assert_called_once()
    login.assert_not_called()


def test_workflow_preflights_session_without_replaying(monkeypatch, tmp_path):
    store = _saved_store(tmp_path)
    client = Mock()
    ensure = Mock(return_value={"status": "valid"})
    workflow = Mock(return_value={"profiles": []})
    monkeypatch.setattr(lixiao, "_ensure_session", ensure)
    monkeypatch.setattr(lixiao, "_run_company_profiles", workflow)
    args = lixiao.build_parser().parse_args(
        ["workflow", "company-profile", "--id", "company-id"]
    )

    assert lixiao._run_workflow(args, store, client) == {"profiles": []}
    ensure.assert_called_once_with(client, store, ttocr_url=None)
    workflow.assert_called_once_with(args, client)


def test_auth_status_dry_run_does_not_read_store():
    store = Mock()
    store.summary.side_effect = AssertionError("credentials were read")
    client = Mock()
    args = lixiao.build_parser().parse_args(["--dry-run", "auth", "status"])

    result = lixiao._run_auth(args, store, client)

    assert result["operation"] == "auth-status"
    assert result["credentials_read"] is False
    store.summary.assert_not_called()
