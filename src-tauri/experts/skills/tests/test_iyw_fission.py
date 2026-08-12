import asyncio
import json
import sys
from pathlib import Path

import pytest


SCRIPTS_DIR = Path(__file__).parents[1] / "iyw-image-workflows" / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))

from iyw_commerce import build_parser, run_command  # noqa: E402
from iyw_fission_core import (  # noqa: E402
    _select_fission_models,
    create_fission_tasks,
)
from iyw_image import IywError  # noqa: E402


def _options(*platforms: str) -> list[dict[str, str]]:
    return [
        {"label": f"分身 {platform}", "value": platform}
        for platform in platforms
    ]


def _dry_run(*extra_args: str) -> dict:
    args = build_parser().parse_args(
        [
            "fission-generate",
            "--prompt",
            "产品设计草图",
            *extra_args,
            "--dry-run",
            "--no-progress",
        ]
    )
    return asyncio.run(run_command(args))


class _FakeClient:
    def __init__(self, response: dict | None = None):
        self.response = response or {}
        self.requests = []

    async def request(self, path, payload, *, dry_run=False):
        self.requests.append((path, payload, dry_run))
        if dry_run:
            return {"path": path, "body": payload}
        return self.response


def test_default_selection_prefers_platform_four():
    payloads = _select_fission_models(_options("1", "4", "8"))

    assert [item["platform"] for item in payloads] == ["4"]


def test_default_selection_falls_back_to_first_live_platform():
    payloads = _select_fission_models(_options("8", "1"))

    assert [item["platform"] for item in payloads] == ["8"]


def test_comparison_selection_puts_platform_four_first():
    payloads = _select_fission_models(
        _options("8", "1", "4"), compare_platforms=True
    )

    assert [item["platform"] for item in payloads] == ["4", "8", "1"]


def test_comparison_selection_preserves_order_without_platform_four():
    payloads = _select_fission_models(
        _options("8", "1"), compare_platforms=True
    )

    assert [item["platform"] for item in payloads] == ["8", "1"]


@pytest.mark.parametrize(
    ("options", "message"),
    [
        (
            [{"label": "分身未知", "value": "999"}],
            "unsupported live fission configuration",
        ),
        ([], "no supported fission models are available"),
        (
            [
                {"label": "分身 A", "value": "4"},
                {"label": "分身 B", "value": "4"},
            ],
            "duplicate",
        ),
    ],
)
def test_invalid_live_configuration_is_rejected(options, message):
    with pytest.raises(IywError, match=message):
        _select_fission_models(options)


def test_compare_platforms_is_opt_in():
    parser = build_parser()

    default_args = parser.parse_args(
        ["fission-generate", "--prompt", "产品设计草图"]
    )
    compare_args = parser.parse_args(
        [
            "fission-generate",
            "--prompt",
            "产品设计草图",
            "--compare-platforms",
        ]
    )

    assert default_args.compare_platforms is False
    assert compare_args.compare_platforms is True


def test_default_dry_run_only_sends_platform_four():
    result = _dry_run()

    assert [item["platform"] for item in result["body"]["models"]] == ["4"]


def test_comparison_dry_run_sends_all_platforms_with_four_first():
    result = _dry_run("--compare-platforms")

    platforms = [item["platform"] for item in result["body"]["models"]]
    assert platforms == ["4", "1", "8", "5", "2", "12"]


def test_live_configuration_selection_reaches_batch_request():
    api_client = _FakeClient(
        {"groupId": "group", "tasks": [{"code": 1, "data": {"taskId": "1"}}]}
    )
    config_client = _FakeClient(
        {"model_options": json.dumps(_options("8", "4", "1"))}
    )

    result = asyncio.run(
        create_fission_tasks(
            api_client,
            config_client,
            "产品设计草图",
            compare_platforms=True,
        )
    )

    assert result["status"] == "queued"
    assert [model["platform"] for model in api_client.requests[0][1]["models"]] == [
        "4",
        "8",
        "1",
    ]


def test_dry_run_does_not_read_live_configuration():
    api_client = _FakeClient()
    config_client = _FakeClient()

    result = asyncio.run(
        create_fission_tasks(
            api_client,
            config_client,
            "产品设计草图",
            dry_run=True,
        )
    )

    assert config_client.requests == []
    assert [model["platform"] for model in result["body"]["models"]] == ["4"]
