import asyncio
import json
import sys
from pathlib import Path

import pytest

SCRIPTS_DIR = Path(__file__).parents[1] / "iyw-image-workflows" / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))

from iyw_commerce import build_parser, run_command
from iyw_commerce_core import _validate_generate_payload
from iyw_image import IywError

HTTPS = "https://example.com/image.png"


def _invoke_args(tmp_path, tool_name, image_urls):
    payload_file = tmp_path / f"{tool_name}.json"
    payload_file.write_text(
        json.dumps(
            {
                "imageUrls": image_urls,
                "prompt": "生成系列作品",
                "toolName": tool_name,
                "batchSize": 4,
                "modelChannel": 9,
            }
        ),
        encoding="utf-8",
    )
    return build_parser().parse_args(
        [
            "invoke",
            "g_tools_generate_image",
            "--input-file",
            str(payload_file),
            "--dry-run",
            "--no-progress",
        ]
    )


def test_generic_generate_operation_rejects_unknown_tool_name():
    with pytest.raises(IywError, match="unsupported"):
        _validate_generate_payload({"toolName": "unknown", "imageUrls": HTTPS})


@pytest.mark.parametrize("tool_name", ["variation", "extend"])
def test_generic_invoke_fixes_model_and_batch(tmp_path, tool_name):
    result = asyncio.run(run_command(_invoke_args(tmp_path, tool_name, HTTPS)))

    assert result["body"]["batchSize"] == 1
    assert result["body"]["modelChannel"] == 2


def test_generic_mix_invoke_fixes_model_and_keeps_order(tmp_path):
    images = [HTTPS, "https://example.com/second.png"]

    result = asyncio.run(run_command(_invoke_args(tmp_path, "mix", images)))

    assert result["body"]["modelChannel"] == 2
    assert result["body"]["imageUrls"] == images


@pytest.mark.parametrize(
    "query",
    [
        "X-Amz-Signature=secret",
        "x-oss-signature=secret",
        "q-sign-algorithm=sha1",
        "X-Goog-Credential=secret",
    ],
)
def test_check_image_rejects_signed_urls_before_dry_run(query):
    args = build_parser().parse_args(
        ["check-image", "--image-url", f"{HTTPS}?{query}", "--dry-run"]
    )

    with pytest.raises(IywError, match="signed URL"):
        asyncio.run(run_command(args))


def test_check_image_rejects_signed_url_with_leading_whitespace():
    signed_url = f"  {HTTPS}?X-Amz-Signature=secret"
    args = build_parser().parse_args(
        ["check-image", "--image-url", signed_url, "--dry-run"]
    )

    with pytest.raises(IywError, match="signed URL"):
        asyncio.run(run_command(args))


def test_generic_invoke_rejects_signed_url_with_leading_whitespace(tmp_path):
    signed_url = f"  {HTTPS}?X-Amz-Signature=secret"
    args = _invoke_args(tmp_path, "variation", signed_url)

    with pytest.raises(IywError, match="signed URL"):
        asyncio.run(run_command(args))
