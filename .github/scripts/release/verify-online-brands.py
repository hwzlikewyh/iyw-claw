"""在临时 CI 桌面中通过正式向导界面安装两个品牌并检查数据隔离。"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

VERSION = "0.1.374"
RELEASE = "384624258478997504"
HOME = Path.home()
TEMP = Path(os.environ["RUNNER_TEMP"])
INPUT = Path(os.environ["ACCEPTANCE_INPUT"])
IS_WINDOWS = sys.platform == "win32"
IS_MAC = sys.platform == "darwin"
REPORT = {"version": VERSION, "releaseId": RELEASE, "checks": []}


def command(args, timeout=60, env=None):
    result = subprocess.run(args, capture_output=True, text=True, encoding="utf-8",
                            errors="replace", timeout=timeout, env=env)
    if result.returncode:
        raise RuntimeError(f"{Path(args[0]).name} exited {result.returncode}: " + result.stderr[-900:])
    return result.stdout.strip()


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def paths(brand, name):
    data = HOME / (".iyw-claw" if brand == "iyw" else ".iyw-claw-" + brand)
    if IS_WINDOWS:
        root = Path(os.environ["LOCALAPPDATA"]) / name
        return root, root / "app/iyw-claw.exe", root / "app/iyw-environment.exe", data
    if IS_MAC:
        root = HOME / "Applications" / (name + ".app")
        return root, root / "Contents/MacOS/iyw-claw", root / "Contents/MacOS/iyw-environment", data
    root = HOME / ".local/share" / ("iyw-claw" if brand == "iyw" else "iyw-claw-" + brand)
    return root, root / "app/usr/bin/iyw-claw", root / "app/usr/bin/iyw-environment", data


def wizard(brand, name):
    prefix = "YuanAssistant-Setup" if brand == "iyw" else brand + "-Setup"
    folder = next(path for path in INPUT.iterdir() if path.name.endswith("-" + brand))
    for line in (folder / "SHA256SUMS.txt").read_text().splitlines():
        expected, filename = line.split(maxsplit=1)
        assert digest(folder / filename.lstrip("*")) == expected, "Installer digest mismatch"
    if IS_WINDOWS:
        exe = folder / (prefix + "-windows.exe")
        env = dict(os.environ, VERIFY_INSTALLER=str(exe))
        command(["pwsh", "-NoProfile", "-Command",
                 "$s=Get-AuthenticodeSignature -LiteralPath $env:VERIFY_INSTALLER;"
                 "if($s.Status -ne 'Valid' -or !$s.TimeStamperCertificate){exit 1}"], env=env)
        return exe
    if IS_MAC:
        destination = TEMP / ("wizard-" + brand)
        command(["ditto", "-x", "-k", str(folder / (prefix + "-macos-universal.zip")), str(destination)])
        bundle = next(destination.rglob(name + "安装向导.app"))
        command(["codesign", "--verify", "--deep", "--strict", "-R",
                 '=anchor apple generic and certificate leaf[subject.OU] = "393H57RCN6"', str(bundle)])
        return bundle / "Contents/MacOS" / (name + "安装向导")
    destination = TEMP / ("wizard-" + brand)
    destination.mkdir()
    command(["tar", "-xzf", str(folder / (prefix + "-linux-" + os.environ["ACCEPTANCE_ARCH"] + ".tar.gz")),
             "-C", str(destination)])
    filename = "yuan-assistant-installer" if brand == "iyw" else "iyw-" + brand + "-installer"
    return next(p for p in destination.rglob(filename) if p.is_file())


def start_install(exe, repeated):
    output = (TEMP / ("wizard-" + exe.name + ".log")).open("w", encoding="utf-8")
    process = subprocess.Popen([str(exe)], stdout=output, stderr=output)
    time.sleep(4)
    assert process.poll() is None, "Installer closed before interaction"
    if IS_WINDOWS:
        from pywinauto import Application
        app = Application(backend="uia").connect(process=process.pid, timeout=30)
        window = app.top_window()
        window.set_focus()
        window.type_keys("{ENTER}", set_foreground=True)
        if not repeated:
            time.sleep(2)
            window.type_keys("{ENTER}", set_foreground=True)
    elif IS_MAC:
        script = ('tell application "System Events"\n'
                  f'set p to first application process whose unix id is {process.pid}\n'
                  'set frontmost of p to true\nkey code 36\ndelay 2\nkey code 36\nend tell')
        command(["osascript", "-e", script])
    else:
        window = command(["xdotool", "search", "--sync", "--onlyvisible", "--pid", str(process.pid)])
        window = window.splitlines()[0]
        command(["xdotool", "windowfocus", "--sync", window])
        command(["xdotool", "key", "--clearmodifiers", "Return"])
        time.sleep(2)
        command(["xdotool", "key", "--clearmodifiers", "Return"])
    return process, output


def wait_installed(process, root, executable, data, prior_stages, started):
    deadline = time.monotonic() + 35 * 60
    stages = set()
    while time.monotonic() < deadline:
        stages.update(set(Path(tempfile.gettempdir()).glob("yuan-setup-*")) - prior_stages)
        errors = [p for stage in stages for p in stage.glob("installer-error.log")]
        logs = [p for p in (data / "logs/installer").glob("*") if p.stat().st_mtime >= started]
        errors.extend(p / "installer-error.log" for p in logs if (p / "installer-error.log").exists())
        if errors:
            text = errors[0].read_text(encoding="utf-8-sig", errors="replace")
            raise RuntimeError("Installer failure: " + text[-1800:].replace(str(HOME), "<home>"))
        snapshot = data / "inventory/environment-current.json"
        receipt = root / ("installer/direct-current.json" if IS_WINDOWS else "installed-release.json")
        success = bool(logs) and all(not p.exists() for p in stages)
        if IS_WINDOWS or not IS_MAC:
            success = receipt.exists() and receipt.stat().st_mtime >= started
        if executable.is_file() and snapshot.is_file() and success:
            time.sleep(3)
            return
        if process.poll() is not None:
            raise RuntimeError("Installer exited before a successful installation")
        time.sleep(3)
    raise RuntimeError("Installation did not finish within 35 minutes")


def verify_brand(brand, name, root, exe, helper, data):
    identity = json.loads(command([str(exe), "--installation-brand"], timeout=30))
    assert identity["brandId"] == brand and identity["displayName"] == name
    assert identity["productKey"] == "iyw-claw" and identity["hasLogo"]
    assert identity["userDirectory"] == data.name
    config = Path(str(root) + ".branding") / "branding.json" if IS_MAC else root / "config/branding.json"
    installed = json.loads(config.read_text(encoding="utf-8-sig"))
    assert installed["releaseId"] == RELEASE and installed["brandId"] == brand
    snapshot = json.loads((data / "inventory/environment-current.json").read_text(encoding="utf-8-sig"))
    assert snapshot["pcVersion"] == VERSION
    components = {item["componentId"] for item in snapshot["components"]}
    assert {"node", "git", "uv"} <= components
    env = dict(os.environ, IYW_CLAW_BRAND_HOME=str(data))
    diagnosis = json.loads(command([str(helper), "diagnose", "--json"], timeout=120, env=env))
    assert diagnosis["status"] == "healthy"
    if not IS_MAC and not IS_WINDOWS:
        launcher = HOME / ".local/share/applications" / (identity["identifier"] + ".desktop")
        assert ("Name=" + name) in launcher.read_text()
    REPORT["checks"].append({"brand": brand, "stage": "installed", "components": sorted(components),
                             "sha256": digest(exe), "userDirectory": data.name})


def install_brand(brand, name):
    root, exe, helper, data = paths(brand, name)
    assert not root.exists() and not data.exists(), "Acceptance requires an empty CI profile"
    installer = wizard(brand, name)
    markers = []
    for repeated in (False, True):
        prior_stages = set(Path(tempfile.gettempdir()).glob("yuan-setup-*"))
        started = time.time()
        process, output = start_install(installer, repeated)
        try:
            wait_installed(process, root, exe, data, prior_stages, started)
            verify_brand(brand, name, root, exe, helper, data)
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=20)
            output.close()
        if not repeated:
            for folder in ("config", "plugins", "browser/profiles"):
                marker = data / folder / "brand-acceptance.txt"
                marker.parent.mkdir(parents=True, exist_ok=True)
                marker.write_text(brand, encoding="utf-8")
                markers.append(marker)
        else:
            assert all(marker.read_text() == brand for marker in markers)
            REPORT["checks"].append({"brand": brand, "stage": "repeat-install-preserves-data"})
    return root, exe, data


def verify_running(installed):
    processes = []
    try:
        for root, exe, data in installed:
            launch = exe if IS_WINDOWS or IS_MAC else root / "app/AppRun"
            output = (TEMP / ("app-" + data.name + ".log")).open("w")
            process = subprocess.Popen([str(launch)], stdout=output, stderr=output)
            processes.append((process, output))
        time.sleep(15)
        assert all(process.poll() is None for process, _ in processes), "Both branded apps must remain running"
        REPORT["checks"].append({"stage": "both-brands-running", "isolatedHomes": [p[2].name for p in installed]})
    finally:
        for process, output in processes:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=20)
                except subprocess.TimeoutExpired:
                    process.kill()
            output.close()


try:
    assert os.environ.get("RUNNER_ENVIRONMENT") == "github-hosted", "Use a disposable hosted runner"
    installed = [install_brand(brand, name) for brand, name in (("iyw", "原助理"), ("yun", "云助理"))]
    assert digest(installed[0][1]) == digest(installed[1][1]), "Brands must share identical PC executables"
    verify_running(installed)
    REPORT["status"] = "passed"
except Exception as error:
    REPORT["status"] = "failed"
    REPORT["error"] = str(error).replace(str(HOME), "<home>")
    raise
finally:
    (TEMP / "brand-installation-result.json").write_text(json.dumps(REPORT, ensure_ascii=False, indent=2), encoding="utf-8")
