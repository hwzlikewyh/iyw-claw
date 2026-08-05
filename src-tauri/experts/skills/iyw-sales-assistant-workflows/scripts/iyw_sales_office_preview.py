from __future__ import annotations

import json
import os
import shutil
import subprocess
import tempfile
from collections.abc import Callable
from pathlib import Path

WINDOWS_BROWSERS = (
    ("ProgramFiles", "Google/Chrome/Application/chrome.exe"),
    ("ProgramFiles(x86)", "Google/Chrome/Application/chrome.exe"),
    ("ProgramFiles(x86)", "Microsoft/Edge/Application/msedge.exe"),
    ("ProgramFiles", "Microsoft/Edge/Application/msedge.exe"),
)


class OfficePreviewError(OSError):
    pass


def _browser_path() -> str | None:
    if os.name == "nt":
        for variable, suffix in WINDOWS_BROWSERS:
            root = os.environ.get(variable)
            if root and (candidate := Path(root) / suffix).is_file():
                return str(candidate)
    for name in ("google-chrome", "chromium", "msedge", "firefox"):
        if candidate := shutil.which(name):
            return candidate
    return None


def _windows_browser(browser: str, arguments: list[str]) -> int:
    powershell = shutil.which("pwsh") or shutil.which("powershell")
    if not powershell:
        return 1
    environment = os.environ.copy()
    environment.update(
        {
            "IYW_BROWSER_EXE": browser,
            "IYW_BROWSER_ARGS": json.dumps(arguments),
        }
    )
    script = (
        "$items = ConvertFrom-Json $env:IYW_BROWSER_ARGS; "
        "$process = Start-Process -FilePath $env:IYW_BROWSER_EXE "
        "-ArgumentList $items -Wait -PassThru -WindowStyle Hidden; "
        "exit $process.ExitCode"
    )
    return subprocess.run(
        [powershell, "-NoProfile", "-NonInteractive", "-Command", script],
        check=False,
        timeout=60,
        env=environment,
    ).returncode


def _run_browser(browser: str, profile: Path, preview: Path, html: Path) -> int:
    arguments = [
        "--headless=new",
        "--disable-gpu",
        "--no-first-run",
        "--no-default-browser-check",
        f"--user-data-dir={profile}",
        "--window-size=1600,1200",
        f"--screenshot={preview}",
        html.resolve().as_uri(),
    ]
    if os.name == "nt":
        return _windows_browser(browser, arguments)
    return subprocess.run(
        [browser, *arguments], check=False, timeout=60
    ).returncode


def fallback_screenshot(
    path: Path,
    preview: Path,
    render_html: Callable[[list[str]], str],
) -> None:
    browser = _browser_path()
    if not browser:
        raise OfficePreviewError("未找到可用于 Office 预览的本机浏览器")
    profile = Path(tempfile.mkdtemp(prefix=".office-preview-", dir=path.parent))
    html = profile / "preview.html"
    browser_preview = profile / "preview.png"
    try:
        render_html(["view", str(path), "html", "-o", str(html)])
        if _run_browser(browser, profile, browser_preview, html):
            raise OfficePreviewError("本机浏览器未能生成 Office 预览图")
        if not browser_preview.is_file() or not browser_preview.stat().st_size:
            raise OfficePreviewError("本机浏览器未能生成 Office 预览图")
        shutil.copy2(browser_preview, preview)
    finally:
        shutil.rmtree(profile, ignore_errors=True)
