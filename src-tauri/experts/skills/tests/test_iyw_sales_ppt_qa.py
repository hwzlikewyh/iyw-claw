import subprocess
import sys
from pathlib import Path

import pytest


SCRIPTS_DIR = Path(__file__).parents[1] / "iyw-sales-assistant-workflows" / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))

from iyw_sales_ppt import (  # noqa: E402
    PresentationToolError,
    _run_presentation_qa,
)


def completed(args, returncode=0, stdout="", stderr=""):
    return subprocess.CompletedProcess(args, returncode, stdout, stderr)


def test_presentation_qa_renders_then_checks_overflow(tmp_path):
    output = tmp_path / "company.pptx"
    output.write_bytes(b"pptx")
    commands = []

    def runner(args, **kwargs):
        commands.append((args, kwargs))
        if Path(args[1]).name == "render_slides.py":
            rendered = Path(args[args.index("--output_dir") + 1])
            rendered.mkdir()
            (rendered / "slide-1.png").write_bytes(b"png")
        return completed(args, stdout="Test passed. No overflow detected.")

    _run_presentation_qa(output, tmp_path / "skill", runner)

    assert [Path(item[0][1]).name for item in commands] == ["render_slides.py", "slides_test.py"]
    assert commands[0][1]["env"]["TEMP"].startswith(str(tmp_path))
    assert not list(tmp_path.glob(".iyw-ppt-qa-*"))


def test_presentation_qa_rejects_render_failure(tmp_path):
    output = tmp_path / "company.pptx"
    output.write_bytes(b"pptx")

    def runner(args, **_kwargs):
        return completed(args, returncode=1, stderr="render failed")

    with pytest.raises(PresentationToolError, match="渲染检查失败.*render failed"):
        _run_presentation_qa(output, tmp_path / "skill", runner)


def test_presentation_qa_rejects_empty_render(tmp_path):
    output = tmp_path / "company.pptx"
    output.write_bytes(b"pptx")

    def runner(args, **_kwargs):
        Path(args[args.index("--output_dir") + 1]).mkdir()
        return completed(args)

    with pytest.raises(PresentationToolError, match="未生成任何幻灯片图片"):
        _run_presentation_qa(output, tmp_path / "skill", runner)


def test_presentation_qa_rejects_zero_exit_overflow_error(tmp_path):
    output = tmp_path / "company.pptx"
    output.write_bytes(b"pptx")

    def runner(args, **_kwargs):
        if Path(args[1]).name == "render_slides.py":
            rendered = Path(args[args.index("--output_dir") + 1])
            rendered.mkdir()
            (rendered / "slide-1.png").write_bytes(b"png")
            return completed(args)
        return completed(
            args,
            stdout="ERROR: Slides with content overflowing original canvas: 2",
            stderr="renderer warning",
        )

    with pytest.raises(PresentationToolError, match="越界检查失败.*ERROR:"):
        _run_presentation_qa(output, tmp_path / "skill", runner)


def test_material_page_distinguishes_non_image_files():
    script = (SCRIPTS_DIR / "iyw_sales_ppt.mjs").read_text(encoding="utf-8")
    assert "已取得非图片资料" in script
    assert "资料来源未提供" in script
    assert "!isMaterialImage(item)" in script
    assert "imageMaterials" in script
