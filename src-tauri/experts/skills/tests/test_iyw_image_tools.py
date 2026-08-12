import sys
import asyncio
import json
from pathlib import Path

import pytest


SCRIPTS_DIR = Path(__file__).parents[1] / "iyw-image-workflows" / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))

from iyw_image import IywError  # noqa: E402
from iyw_tool_core import TOOL_OPERATIONS, validate_tool_payload  # noqa: E402
from iyw_commerce import build_parser, run_command  # noqa: E402


HTTPS = "https://example.com/image.png"


def test_all_fixed_tools_have_operations():
    expected = {
        "variation", "extend", "mix", "pattern-apply", "free-imitation",
        "material-product", "ip-apply", "edit", "outpaint", "super-resolution",
        "split-layers", "separate-layers", "enhance", "extract-pattern",
        "repeat-horizontal", "convert", "line-extraction", "color-transfer",
        "image-to-3d", "video", "model-scene",
    }
    assert set(TOOL_OPERATIONS) == expected
    assert TOOL_OPERATIONS["variation"] == "g_tools_generate_image"
    assert TOOL_OPERATIONS["outpaint"] == "outpainting"
    assert TOOL_OPERATIONS["video"] == "videoGenerator"


def test_variation_sets_fixed_tool_name_and_accepts_one_image():
    payload = {"imageUrls": HTTPS, "prompt": "改成蓝色", "batchSize": 4}

    operation = validate_tool_payload("variation", payload)

    assert operation == "g_tools_generate_image"
    assert payload["toolName"] == "variation"
    assert payload["imageUrls"] == HTTPS
    assert payload["batchSize"] == 1


def test_extend_sets_fixed_tool_name_and_single_batch():
    payload = {"imageUrls": HTTPS, "prompt": "延伸春夏趋势系列"}

    operation = validate_tool_payload("extend", payload)

    assert operation == "g_tools_generate_image"
    assert payload["toolName"] == "extend"
    assert payload["imageUrls"] == HTTPS
    assert payload["batchSize"] == 1


def test_mix_requires_two_to_ten_images():
    with pytest.raises(IywError, match="mix requires"):
        validate_tool_payload("mix", {"imageUrls": [HTTPS], "prompt": "融合"})


def test_pattern_and_material_tools_accept_captured_metadata_shapes():
    images = [HTTPS, "https://example.com/material.png"]
    pattern = {
        "imageUrls": images,
        "product": [{"imageUrl": images[0]}],
        "material": [{"imageUrl": images[1]}],
        "prompt": "应用图案",
    }
    material = {
        "imageUrls": images,
        "product": {"imageUrl": images[0]},
        "material": [{"imageUrl": images[1]}],
        "prompt": "配辅生款",
    }

    assert validate_tool_payload("pattern-apply", pattern) == "g_tools_generate_image"
    assert validate_tool_payload("material-product", material) == "g_tools_generate_image"
    assert pattern["toolName"] == "iyw_tu"
    assert material["toolName"] == "user_product"


def test_tool_rejects_non_https_image():
    with pytest.raises(IywError, match="HTTPS"):
        validate_tool_payload("variation", {"imageUrls": "http://example.com/a.png", "prompt": "改款"})


@pytest.mark.parametrize("field", ["token", "Cookie", "Authorization", "securityKey"])
def test_tool_rejects_nested_sensitive_fields(field):
    payload = {"imageUrls": HTTPS, "prompt": "改款", "jsonData": {field: "secret"}}

    with pytest.raises(IywError, match="sensitive field"):
        validate_tool_payload("variation", payload)


def test_tool_rejects_signed_image_urls():
    payload = {"imageUrls": f"{HTTPS}?Expires=1&Signature=secret", "prompt": "改款"}

    with pytest.raises(IywError, match="signed URL"):
        validate_tool_payload("variation", payload)


def test_tool_rejects_nested_http_image_urls():
    payload = {
        "imageUrls": [HTTPS, "https://example.com/material.png"],
        "product": {"imageUrl": HTTPS},
        "material": [{"imageUrl": "http://example.com/material.png"}],
        "prompt": "配辅生款",
    }

    with pytest.raises(IywError, match="HTTPS"):
        validate_tool_payload("material-product", payload)


def test_ip_apply_rejects_http_image_in_json_data():
    payload = {
        "imageUrls": HTTPS,
        "product": {"imageUrl": HTTPS},
        "jsonData": {"img": "http://example.com/insecure.png"},
        "prompt": "应用 IP",
    }

    with pytest.raises(IywError, match="HTTPS"):
        validate_tool_payload("ip-apply", payload)


def test_repeat_horizontal_preserves_single_image_array():
    payload = {"imageUrls": [HTTPS]}

    operation = validate_tool_payload("repeat-horizontal", payload)

    assert operation == "g_tools_generate_image"
    assert payload["toolName"] == "return_leftright"
    assert payload["imageUrls"] == [HTTPS]


def test_generic_generate_operation_rejects_unknown_tool_name():
    from iyw_commerce_core import _validate_generate_payload

    with pytest.raises(IywError, match="unsupported"):
        _validate_generate_payload({"toolName": "unknown", "imageUrls": HTTPS})


@pytest.mark.parametrize("tool_name", ["variation", "extend"])
def test_generic_invoke_forces_single_batch(tmp_path, tool_name):
    payload_file = tmp_path / f"{tool_name}.json"
    payload_file.write_text(
        json.dumps(
            {
                "imageUrls": HTTPS,
                "prompt": "生成一张完整联图",
                "toolName": tool_name,
                "batchSize": 4,
            }
        ),
        encoding="utf-8",
    )
    args = build_parser().parse_args(
        [
            "invoke",
            "g_tools_generate_image",
            "--input-file",
            str(payload_file),
            "--dry-run",
            "--no-progress",
        ]
    )

    result = asyncio.run(run_command(args))

    assert result["body"]["batchSize"] == 1


def test_generic_invoke_rejects_sensitive_payload(tmp_path):
    payload_file = tmp_path / "unsafe.json"
    payload_file.write_text(
        json.dumps({"imageUrls": HTTPS, "prompt": "改款", "toolName": "variation", "token": "secret"}),
        encoding="utf-8",
    )
    args = build_parser().parse_args(
        [
            "invoke",
            "g_tools_generate_image",
            "--input-file",
            str(payload_file),
            "--dry-run",
            "--no-progress",
        ]
    )

    with pytest.raises(IywError, match="sensitive field"):
        asyncio.run(run_command(args))


def test_specialized_payloads_validate_enums():
    payload = {
        "reference": HTTPS,
        "prompt": "产品转视频",
        "ratio": "16:9",
        "duration": 10,
        "mode": "normal",
    }
    assert validate_tool_payload("video", payload) == "videoGenerator"

    with pytest.raises(IywError, match="2 or 4"):
        validate_tool_payload("super-resolution", {"reference": HTTPS, "upscale": 3})


def test_unknown_tool_is_rejected():
    with pytest.raises(IywError, match="unsupported tool"):
        validate_tool_payload("not-a-tool", {})


def test_tool_cli_dispatches_fixed_operation_in_dry_run(tmp_path):
    payload_file = tmp_path / "variation.json"
    payload_file.write_text(json.dumps({"imageUrls": HTTPS, "prompt": "改款"}), encoding="utf-8")
    args = build_parser().parse_args(
        [
            "tool",
            "variation",
            "--input-file",
            str(payload_file),
            "--dry-run",
            "--no-progress",
        ]
    )

    result = asyncio.run(run_command(args))

    assert result["url"] == "https://gateway.iyw.cn/ai-application/api/commerce/g_tools_generate_image"
    assert result["body"]["toolName"] == "variation"
