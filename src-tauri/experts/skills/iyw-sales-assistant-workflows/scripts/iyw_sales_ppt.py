from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Callable

from iyw_sales_batch_office import (
    _activity_entries,
    _business_info,
    _market_text,
)

PPT_SCRIPT = Path(__file__).with_name("iyw_sales_ppt.mjs")
PPT_MODULES = (PPT_SCRIPT, PPT_SCRIPT.with_name("iyw_sales_ppt_theme.mjs"))


class PresentationToolError(RuntimeError):
    """artifact-tool 生成 PPT 失败。"""


def _clip(value: object, limit: int = 220) -> str:
    text = str(value or "").strip()
    return text if len(text) <= limit else f"{text[: limit - 1]}..."


def _analysis(products: list[dict[str, Any]]) -> dict[str, str]:
    summaries: list[str] = []
    angles: list[str] = []
    markets: list[str] = []
    for product in products:
        analysis = product.get("analysis")
        if isinstance(analysis, str) and analysis.strip():
            summaries.append(analysis.strip())
        elif isinstance(analysis, dict):
            for key, target in (("summary", summaries), ("selling_points", summaries), ("sales_angle", angles), ("target_market", markets)):
                value = analysis.get(key)
                if value:
                    target.append(str(value).strip())
    return {
        "summary": _clip("、".join(dict.fromkeys(summaries)) or "暂无图片分析"),
        "angle": _clip("、".join(dict.fromkeys(angles)) or "暂无销售切入点"),
        "market": _clip("、".join(dict.fromkeys(markets)) or "未提供"),
    }


def _activity_payload(record: dict[str, Any]) -> dict[str, list[str]]:
    result: dict[str, list[str]] = {}
    for group, windows in _activity_entries(record).items():
        selected = windows["six"] or windows["year"]
        prefix = "近一年补充：" if not windows["six"] and windows["year"] else ""
        lines = []
        for item in selected[:4]:
            detail = item.get("evidence") or item.get("job_title") or item.get("event_name") or item.get("name") or "有记录"
            date = str(item.get("observed_at") or "")[:10] or "日期未知"
            lines.append(f"{prefix}{detail}（{date}）")
        result[group] = lines or ["无有效记录"]
    return result


def _contact_payload(plan: dict[str, Any]) -> list[dict[str, str]]:
    return [
        {
            "name": str(item.get("name") or "未命名联系人"),
            "role": str(item.get("role") or "角色未提供"),
            "phone": str(item.get("phone") or "电话未提供"),
            "source": str(item.get("source") or "输入记录"),
        }
        for item in plan.get("contacts", {}).get("selected", [])[:3]
        if isinstance(item, dict)
    ]


def _material_payload(plan: dict[str, Any]) -> list[dict[str, str]]:
    materials: list[dict[str, str]] = []
    for item in plan.get("materials", {}).get("selected", []):
        if not isinstance(item, dict) or not item.get("local_path"):
            continue
        path = Path(str(item["local_path"]))
        materials.append(
            {
                "type": str(item.get("type") or "销售资料"),
                "name": str(item.get("name") or path.stem),
                "local_path": str(path),
                "source": str(item.get("source") or "资料来源未提供"),
            }
        )
    return materials


def _product_payload(products: list[dict[str, Any]]) -> list[dict[str, str]]:
    result: list[dict[str, str]] = []
    for item in products[:10]:
        if not isinstance(item, dict) or not item.get("local_path"):
            continue
        result.append(
            {
                "name": str(item.get("name") or "店铺产品"),
                "local_path": str(item["local_path"]),
                "source": str(item.get("source") or item.get("product_url") or "店铺产品记录"),
            }
        )
    return result


def build_ppt_input(plan: dict[str, Any]) -> dict[str, Any]:
    record = plan["record"]
    company = record["company"]
    products = plan.get("products", {}).get("selected", [])
    analysis = _analysis(products)
    product_images = _product_payload(products)
    run = record["run"]
    return {
        "company_name": str(company.get("name") or "未命名公司"),
        "company_source": str(company.get("source") or "lixiao:company-profile"),
        "business_info": _business_info(company),
        "market_keywords": _market_text(record, analysis),
        "shop_url": str(
            company.get("shop_url")
            or next((item.get("store_url") for item in products if item.get("store_url")), None)
            or "店铺链接未提供"
        ),
        "product_urls": [
            str(item.get("product_url"))
            for item in products
            if isinstance(item, dict) and item.get("product_url")
        ][:6],
        "contacts": _contact_payload(plan),
        "products": [
            {
                "name": item["name"],
                "summary": analysis["summary"],
                "angle": analysis["angle"],
            }
            for item in product_images[:3]
        ],
        "product_images": product_images,
        "materials": _material_payload(plan),
        "activities": _activity_payload(record),
        "activity_sources": [
            str(item.get("source"))
            for item in record.get("activities", [])
            if isinstance(item, dict) and item.get("source")
        ],
        "opening_copy": str((record.get("outreach") or {}).get("opening_copy") or "开场白待补"),
        "sales_angle": analysis["angle"],
        "market": str(run.get("market") or "未提供"),
        "as_of": str(run.get("as_of") or "")[:10],
    }


def _presentation_skill_dir() -> Path | None:
    configured = os.environ.get("IYW_PRESENTATIONS_SKILL_DIR")
    if configured:
        candidate = Path(configured)
        if (candidate / "container_tools" / "setup_artifact_tool_workspace.mjs").is_file():
            return candidate
    root = Path.home() / ".codex" / "plugins" / "cache" / "openai-primary-runtime" / "presentations"
    candidates = sorted(root.glob("*") , reverse=True)
    for candidate in candidates:
        if (candidate / "skills" / "presentations" / "container_tools" / "setup_artifact_tool_workspace.mjs").is_file():
            return candidate / "skills" / "presentations"
    return None


def _qa_environment(workspace: Path) -> dict[str, str]:
    environment = os.environ.copy()
    temp_path = str(workspace)
    environment.update({"TEMP": temp_path, "TMP": temp_path, "TMPDIR": temp_path})
    return environment


def _run_qa_command(
    command: list[str],
    stage: str,
    workspace: Path,
    runner: Callable[..., subprocess.CompletedProcess[str]],
) -> subprocess.CompletedProcess[str]:
    result = runner(
        command,
        text=True,
        capture_output=True,
        check=False,
        env=_qa_environment(workspace),
    )
    detail = "\n".join(part.strip() for part in (result.stdout, result.stderr) if part and part.strip())
    if result.returncode or "ERROR:" in detail or "overflowing original canvas" in detail:
        raise PresentationToolError(f"PPT {stage}失败：{detail or f'退出码 {result.returncode}'}")
    return result


def _run_presentation_qa(
    output: Path,
    skill_dir: Path,
    runner: Callable[..., subprocess.CompletedProcess[str]],
) -> None:
    tools_dir = skill_dir / "container_tools"
    with tempfile.TemporaryDirectory(prefix=".iyw-ppt-qa-", dir=output.parent) as qa_name:
        workspace = Path(qa_name)
        qa_pptx = workspace / "presentation.pptx"
        rendered = workspace / "rendered"
        shutil.copy2(output, qa_pptx)
        _run_qa_command(
            [sys.executable, str(tools_dir / "render_slides.py"), str(qa_pptx), "--output_dir", str(rendered)],
            "渲染检查",
            workspace,
            runner,
        )
        if not any(rendered.glob("*.png")):
            raise PresentationToolError("PPT 渲染检查失败：未生成任何幻灯片图片")
        _run_qa_command(
            [sys.executable, str(tools_dir / "slides_test.py"), str(qa_pptx)],
            "越界检查",
            workspace,
            runner,
        )


def validate_company_presentation(
    output_path: str | Path,
    *,
    runner: Callable[..., subprocess.CompletedProcess[str]] = subprocess.run,
) -> Path:
    output = Path(output_path)
    skill_dir = _presentation_skill_dir()
    if skill_dir is None:
        raise PresentationToolError("未找到 presentations Skill 的 PPT 校验环境")
    _run_presentation_qa(output, skill_dir, runner)
    return output


def generate_company_presentation(
    plan: dict[str, Any],
    output_path: str | Path,
    *,
    runner: Callable[..., subprocess.CompletedProcess[str]] = subprocess.run,
) -> Path:
    output = Path(output_path)
    skill_dir = _presentation_skill_dir()
    if skill_dir is None:
        raise PresentationToolError("未找到 presentations Skill 的 artifact-tool 运行环境")
    missing_modules = [str(module) for module in PPT_MODULES if not module.is_file()]
    if missing_modules:
        raise PresentationToolError(f"缺少 PPT 生成器：{', '.join(missing_modules)}")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".iyw-ppt-build-", dir=output.parent) as workspace_name:
        workspace = Path(workspace_name)
        setup = skill_dir / "container_tools" / "setup_artifact_tool_workspace.mjs"
        setup_result = runner(
            ["node", str(setup), "--workspace", str(workspace)],
            text=True,
            capture_output=True,
            check=False,
        )
        if setup_result.returncode:
            raise PresentationToolError((setup_result.stderr or setup_result.stdout).strip())
        for module in PPT_MODULES:
            shutil.copy2(module, workspace / module.name)
        script = workspace / PPT_SCRIPT.name
        payload_path = workspace / "ppt-input.json"
        payload_path.write_text(json.dumps(build_ppt_input(plan), ensure_ascii=False), encoding="utf-8")
        command = [
            "node",
            str(script),
            "--input",
            str(payload_path),
            "--output",
            str(output),
            "--workspace",
            str(workspace),
        ]
        result = runner(command, text=True, capture_output=True, check=False)
        if result.returncode:
            detail = (result.stderr or result.stdout).strip()
            raise PresentationToolError(detail or "artifact-tool PPT 生成失败")
    if not output.is_file() or output.stat().st_size == 0:
        raise PresentationToolError(f"PPT 未生成：{output}")
    return validate_company_presentation(output, runner=runner)
