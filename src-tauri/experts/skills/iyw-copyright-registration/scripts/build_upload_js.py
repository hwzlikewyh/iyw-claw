#!/usr/bin/env python3
"""生成 IYW 版权登记附件上传所需的浏览器 JavaScript。"""

from __future__ import annotations

import argparse
import base64
import json
import mimetypes
from pathlib import Path

SUPPORTED_SUFFIXES = {".jpg", ".jpeg", ".png", ".webp"}
MAX_WORK_BYTES = 20 * 1024 * 1024

TARGETS = {
    "work": """
const target = document.querySelectorAll('input.fileInput')[0];
if (!target) return fail('work input not found');
setInputFiles(target, transfer.files);
target.dispatchEvent(new Event('change', {bubbles: true}));
""",
    "guarantee": """
const label = findLeafLabel('请上传权利保证书');
const target = label && label.parentElement
  ? label.parentElement.querySelector('.getImgContainer') : null;
if (!target) return fail('guarantee drop target not found');
const event = new DragEvent('drop', {bubbles: true, cancelable: true});
Object.defineProperty(event, 'dataTransfer', {value: transfer});
target.dispatchEvent(event);
""",
    "publish": """
const label = findLeafLabel('上传发表凭证');
const target = findInputAbove(label, 'input.fileInput', 6);
if (!target) return fail('publication proof input not found');
setInputFiles(target, transfer.files);
target.dispatchEvent(new Event('change', {bubbles: true}));
""",
}

JS_TEMPLATE = """(() => {
  const kind = __KIND__;
  const filename = __FILENAME__;
  const mime = __MIME__;
  const encoded = __BASE64__;

  function fail(message) {
    return JSON.stringify({ok: false, kind, error: message});
  }

  function findLeafLabel(text) {
    return [...document.querySelectorAll('*')].find(
      (element) => element.children.length === 0 && element.textContent.includes(text)
    ) || null;
  }

  function findInputAbove(element, selector, levels) {
    let container = element ? element.parentElement : null;
    for (let index = 0; container && index < levels; index += 1) {
      const input = container.querySelector(selector);
      if (input) return input;
      container = container.parentElement;
    }
    return null;
  }

  function setInputFiles(input, files) {
    Object.defineProperty(input, 'files', {value: files, configurable: true});
  }

  const binary = atob(encoded);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  const file = new File([bytes], filename, {type: mime});
  const transfer = new DataTransfer();
  transfer.items.add(file);
__TARGET__
  return JSON.stringify({ok: true, kind, filename});
})()
"""


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kind", choices=TARGETS, required=True)
    parser.add_argument("--file", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def validate_file(file_path: Path, kind: str) -> None:
    if not file_path.is_file():
        raise ValueError(f"文件不存在: {file_path}")
    if file_path.suffix.lower() not in SUPPORTED_SUFFIXES:
        raise ValueError(f"不支持的图片格式: {file_path.suffix}")
    if kind == "work" and file_path.stat().st_size > MAX_WORK_BYTES:
        raise ValueError("作品文件不能超过 20 MiB")


def build_script(file_path: Path, kind: str) -> str:
    encoded = base64.b64encode(file_path.read_bytes()).decode("ascii")
    mime = mimetypes.guess_type(file_path.name)[0] or "application/octet-stream"
    replacements = {
        "__KIND__": json.dumps(kind),
        "__FILENAME__": json.dumps(file_path.name, ensure_ascii=False),
        "__MIME__": json.dumps(mime),
        "__BASE64__": json.dumps(encoded),
        "__TARGET__": TARGETS[kind].rstrip(),
    }
    script = JS_TEMPLATE
    for marker, value in replacements.items():
        script = script.replace(marker, value)
    return script


def main() -> int:
    args = parse_args()
    try:
        validate_file(args.file, args.kind)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(build_script(args.file, args.kind), encoding="utf-8")
    except (OSError, ValueError) as error:
        print(json.dumps({"ok": False, "error": str(error)}, ensure_ascii=False))
        return 1
    print(json.dumps({"ok": True, "output": str(args.output)}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
