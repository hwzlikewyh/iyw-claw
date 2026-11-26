import argparse
import hashlib
import json
import subprocess
import urllib.request
import zipfile
from pathlib import Path

PACKAGE_URLS = {
    "77ircloud": "https://oss-openclaw.77ircloud.com/cli_tools/workbuddy/npm/ircloud-cli-workbuddy-1.0.2.tgz",
    "meituan-for-business": "https://s3plus.meituan.net/static-prod01/com.sankuai.sqtaiposp.static.aiteam/sqt-auth-cli.tgz",
    "wps-knowledgebase": "https://personal.wpscdn.cn/app/kwiki-cli/beta/v2.0.3-20260803075737/kwiki-cli-windows-amd64-2.0.3.tgz",
}
WOS_URL = "https://ipaas-huawei-cloud-1252328573.cos.ap-shanghai.myqcloud.com/wai/woscli-windows-amd64.exe"
WOS_SHA = "4ba4d93d6618b159f0d1db5e7b2cf986e9ede1b2177d81b74e21a54764cc7dcd"
UU_SHA = "3352a7cb616668d1be91458337afb4aef1c44e2209a4a40958929d55a9107b0d"
SL_BASE = "https://chat-cdn.tcsl.com.cn/slclaw/workbuddy-cli/"
MGLC_BASE = "https://aigc-assets.mgtv.com/mglc/"
BUFFER_BYTES = 1024 * 1024


def fetch(url):
    with urllib.request.urlopen(url, timeout=60) as response:
        return response.read()


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def download(url, path, expected=None):
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.exists():
        with urllib.request.urlopen(url, timeout=60) as response, path.open("wb") as output:
            while chunk := response.read(BUFFER_BYTES):
                output.write(chunk)
    actual = digest(path)
    if expected and actual != expected:
        raise ValueError(f"供应商制品摘要不匹配：{path.name}")
    return actual


def record(folder, filename, source):
    metadata = {"filename": filename, "sha256": digest(folder / filename), **source}
    (folder / "native.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")
    print(json.dumps({"id": folder.name, "ready": True, "file": filename}), flush=True)


def from_zip(archive, destination, suffix):
    with zipfile.ZipFile(archive) as zipped:
        candidates = [name for name in zipped.namelist() if name.endswith(suffix)]
        if len(candidates) != 1:
            raise ValueError(f"归档入口不唯一：{archive.name}")
        with zipped.open(candidates[0]) as source, destination.open("wb") as output:
            while chunk := source.read(BUFFER_BYTES):
                output.write(chunk)


def mglc(root):
    folder = root / "mglc"
    version = fetch(MGLC_BASE + "latest/version.txt").decode().strip()
    name = f"mglc-{version}-windows-amd64.zip"
    base = MGLC_BASE + f"v{version}/"
    checksums = fetch(base + "checksums.txt").decode().splitlines()
    expected = next(line.split()[0] for line in checksums if line.endswith(name))
    download(base + name, folder / name, expected)
    from_zip(folder / name, folder / "mglc.exe", f"mglc-{version}-windows-amd64.exe")
    record(folder, "mglc.exe", {"version": version, "url": base + name, "archiveSha256": expected})


def uupt(root):
    folder = root / "uupt"
    folder.mkdir(parents=True, exist_ok=True)
    filename = "uupt-open-cli-1.1.0-windows-amd64.zip"
    archive = folder / filename
    if not archive.exists():
        subprocess.run(["gh", "release", "download", "v1.1.0", "-R", "uupt-mcp/uupt-open-cli",
                        "--pattern", filename, "--dir", str(folder)], check=True, timeout=120)
    if digest(archive) != UU_SHA:
        raise ValueError("UU 跑腿制品摘要不匹配")
    from_zip(archive, folder / "uupt-open-cli.exe", "uupt-open-cli.exe")
    record(folder, "uupt-open-cli.exe", {"version": "1.1.0", "archiveSha256": UU_SHA})


def shanlong(root):
    manifest = json.loads(fetch(SL_BASE + "slclaw-cli.manifest.json"))
    entry = "sl-win-x64.exe"
    folder = root / "shanlong-claw"
    download(SL_BASE + entry, folder / "sl-sea.exe", manifest["artifacts"][entry]["sha256"])
    record(folder, "sl-sea.exe", {"version": manifest["version"], "url": SL_BASE + entry})


def wos(root):
    folder = root / "woscli"
    download(WOS_URL, folder / "woscli.exe", WOS_SHA)
    record(folder, "woscli.exe", {"url": WOS_URL})


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    for name, url in PACKAGE_URLS.items():
        try:
            sha = download(url, args.output / name / "package.tgz")
            print(json.dumps({"id": name, "ready": True, "sha256": sha}), flush=True)
        except Exception as error:
            print(json.dumps({"id": name, "ready": False, "error": str(error)}), flush=True)
    for operation in (mglc, uupt, shanlong, wos):
        try:
            operation(args.output)
        except Exception as error:
            print(json.dumps({"id": operation.__name__, "ready": False, "error": str(error)}), flush=True)


if __name__ == "__main__":
    main()
