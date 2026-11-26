"""从已核对的官方分发包提取 Windows 程序，避免执行安装器附带的技能安装。"""
import argparse
import base64
import hashlib
import io
import json
import tarfile
import zipfile

from pathlib import Path
from prepare_sources import download, digest, record

CLOUDHUB_URL = "https://www.yunzhijia.com/mixed/yzj-cli/windows-x64/release/yzj-cli-0.1.8.pak"
CLOUDHUB_SHA = "c6db6cc985269906bc4075cf6a0cd8932c119f7ca764e5d11db45b35799e098a"


def dingtalk(output, metadata):
    package = json.loads((metadata / "dingtalk-workspace-cli.json").read_text(encoding="utf-8"))
    if package["version"] != "1.0.70":
        raise ValueError("钉钉包版本变化，请重新核对后更新适配器")
    folder = output / "dingtalk"
    archive = folder / "package.tgz"
    download(package["dist"]["tarball"], archive)
    algorithm, expected = package["dist"]["integrity"].split("-", 1)
    actual = base64.b64encode(hashlib.new(algorithm, archive.read_bytes()).digest()).decode()
    if actual != expected:
        raise ValueError("钉钉 npm 包完整性校验失败")
    with tarfile.open(archive, "r:gz") as package_archive:
        binary_zip = package_archive.extractfile("package/assets/dws-windows-amd64.zip").read()
    with zipfile.ZipFile(io.BytesIO(binary_zip)) as zipped:
        for name in ("dws.exe", "LICENSE", "NOTICE"):
            (folder / name).write_bytes(zipped.read(name))
    record(folder, "dws.exe", {"version": "1.0.70", "url": package["dist"]["tarball"],
                              "packageIntegrity": package["dist"]["integrity"]})


def cloudhub(output):
    folder = output / "cloudhub"
    archive = folder / "yzj-cli-0.1.8.pak"
    download(CLOUDHUB_URL, archive)
    with zipfile.ZipFile(archive) as zipped:
        metadata = json.loads(zipped.read("update.json"))
        if metadata["version"] != "0.1.8" or metadata["checksum"] != CLOUDHUB_SHA:
            raise ValueError("云之家固定版本或摘要变化，请重新核对")
        (folder / "yzj-cli.exe").write_bytes(zipped.read("yzj-cli.exe"))
    if digest(folder / "yzj-cli.exe") != CLOUDHUB_SHA:
        raise ValueError("云之家二进制摘要不匹配")
    record(folder, "yzj-cli.exe", {"version": "0.1.8", "url": CLOUDHUB_URL})


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--metadata", type=Path, required=True)
    args = parser.parse_args()
    dingtalk(args.output, args.metadata)
    cloudhub(args.output)
