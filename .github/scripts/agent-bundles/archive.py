"""归档当前原生平台的远山受管包，再对解压后的包执行启动验证。"""
import gzip
import hashlib
import json
import subprocess
import tarfile
import zipfile
from pathlib import Path

ROOT = Path('.agent-bundle-work').resolve()
PREFIX = ROOT / 'prefix'
OUTPUT = Path('artifacts/managed-yuanshan').resolve()
CHUNK = 1024 * 1024


def checksum(path):
    value = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(CHUNK), b''):
            value.update(chunk)
    return value.hexdigest()


def write_archive(path, target):
    files = sorted(item for item in PREFIX.rglob('*') if item.is_file())
    if any(item.is_symlink() for item in PREFIX.rglob('*')):
        raise ValueError('bundle still contains symlinks')
    if target == 'windows':
        with zipfile.ZipFile(path, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
            for item in files:
                archive.write(item, item.relative_to(PREFIX).as_posix())
    else:
        with path.open('wb') as out, gzip.GzipFile(fileobj=out, mode='wb', mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode='w') as archive:
                for item in files:
                    info = archive.gettarinfo(str(item), item.relative_to(PREFIX).as_posix())
                    info.uid = info.gid = info.mtime = 0
                    info.uname = info.gname = ''
                    with item.open('rb') as source:
                        archive.addfile(info, source)


def main():
    proof_file = OUTPUT / 'verification.json'
    proof = json.loads(proof_file.read_text('utf-8'))
    target, arch, version = proof['target'], proof['arch'], proof['acpVersion']
    extension = 'zip' if target == 'windows' else 'tar.gz'
    archive_path = OUTPUT / f'claude-acp-{version}-{target}-{arch}.{extension}'
    lock_digest = checksum(PREFIX / 'package-lock.json')
    write_archive(archive_path, target)
    original = ROOT / 'original-prefix'
    PREFIX.rename(original)
    if target == 'windows':
        with zipfile.ZipFile(archive_path) as archive:
            archive.extractall(PREFIX)
    else:
        with tarfile.open(archive_path) as archive:
            archive.extractall(PREFIX, filter='data')
    subprocess.run(['node', '.github/scripts/agent-bundles/prepare.mjs', '--verify-only'], check=True, timeout=180)
    proof.update(archive=archive_path.name, sha256=checksum(archive_path),
                 size=archive_path.stat().st_size, lockSha256=lock_digest, relocatedVerified=True)
    proof_file.write_text(json.dumps(proof, indent=2) + '\n', encoding='utf-8')
    archive_path.with_suffix(archive_path.suffix + '.sha256').write_text(proof['sha256'] + '\n', encoding='ascii')
    print(json.dumps(proof))


if __name__ == '__main__':
    main()
