#!/usr/bin/env python3
"""生成 IYW 版权登记字段预检或单次提交 JavaScript。"""

from __future__ import annotations

import argparse
import json
from datetime import date
from pathlib import Path

JS_TEMPLATE = """(() => {
  const config = __CONFIG__;

  function findComponent() {
    const input = document.querySelector('input[placeholder="请输入作品名称"]');
    let element = input ? input.parentElement : null;
    for (let index = 0; element && index < 15; index += 1) {
      const candidate = element.__vue__;
      if (candidate && candidate._data && candidate._data.modelForm !== undefined) {
        return candidate;
      }
      element = element.parentElement;
    }
    return null;
  }

  function fileCount(component) {
    return component && Array.isArray(component.fileList) ? component.fileList.length : 0;
  }

  function validate(state) {
    const errors = [];
    if (String(state.data.classify) !== '0') errors.push('invalid classify');
    if (!state.data.form.title) errors.push('missing title');
    if (state.workFileLen < 1 || state.workFileLen > 6) errors.push('invalid work file count');
    if (state.showFileLen !== state.workFileLen) errors.push('display image count mismatch');
    const guarantee = state.data.accessory.originalAttachements || {};
    if (!guarantee.url) errors.push('missing guarantee');
    if (!state.model.createStartTime || !state.model.createEndTime) errors.push('missing creation date');
    if (!state.model.createArea) errors.push('missing creation area');
    if (config.published) {
      if (!state.data.form.publishTime) errors.push('missing publication date');
      if (!state.data.form.publishArea) errors.push('missing publication area');
      const proofs = state.data.form.productPubAnnex || [];
      if (proofs.length < 1) errors.push('missing publication proof');
    }
    return errors;
  }

  const view = findComponent();
  if (!view) return JSON.stringify({ok: false, errors: ['component not found']});
  window._dbg = view;
  const data = view._data;
  const model = view.$refs.model && view.$refs.model._data
    ? view.$refs.model._data.modelForm : null;
  if (!model) return JSON.stringify({ok: false, errors: ['model form not found']});

  view.selectType({id: 1, type: '0', name: '美术作品'});
  data.form.creationType = 1;
  data.form.title = config.title;
  data.form.publishStatus = config.published ? 1 : 2;
  if (config.published) {
    data.form.publishTime = config.publishDate;
    data.form.publishArea = config.publishArea;
  }
  data.readed = true;
  data.guarantee = true;
  data.nameAllow = true;
  data.jobDescErr = false;
  data.errHint = false;

  model.templateId = null;
  model.creatNature = 1;
  model.rightsWay = 1;
  model.rightsBelong = config.rightsBelong;
  model.createStartTime = config.creationStart;
  model.createEndTime = config.creationEnd;
  model.createArea = config.creationArea;
  model.copyrightInfo = Array.from({length: 17}, (_, index) => index + 1);

  const workFiles = view.$refs.wirksFile;
  const showFiles = view.$refs.showFile;
  if (!workFiles || typeof workFiles.getFileList !== 'function') {
    return JSON.stringify({ok: false, errors: ['work file component not found']});
  }
  if (!workFiles.__iywOriginalGetFileList) {
    Object.defineProperty(workFiles, '__iywOriginalGetFileList', {
      value: workFiles.getFileList,
      configurable: false
    });
  }
  workFiles.getFileList = function getFileList() {
    return this.__iywOriginalGetFileList.call(this);
  };

  const state = {
    data,
    model,
    workFileLen: fileCount(workFiles),
    showFileLen: fileCount(showFiles)
  };
  const errors = validate(state);
  const result = {
    ok: errors.length === 0,
    action: config.action,
    title: data.form.title,
    publishStatus: data.form.publishStatus,
    workFileLen: state.workFileLen,
    showFileLen: state.showFileLen,
    hasGuarantee: Boolean((data.accessory.originalAttachements || {}).url),
    errors
  };
  if (errors.length > 0 || config.action === 'inspect') return JSON.stringify(result);

  data.form.showImgs = [{img: 'uploaded'}];
  data.isLimit = false;
  view.submit(1);
  result.submitted = true;
  return JSON.stringify(result);
})()
"""


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--action", choices=("inspect", "submit"), required=True)
    parser.add_argument("--title", required=True)
    parser.add_argument("--creation-start", required=True)
    parser.add_argument("--creation-end", required=True)
    parser.add_argument("--creation-area", required=True)
    parser.add_argument("--rights-belong", type=int, choices=(1, 3), required=True)
    parser.add_argument("--published", action="store_true")
    parser.add_argument("--publish-date")
    parser.add_argument("--publish-area")
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def parse_date(value: str, field: str) -> date:
    try:
        return date.fromisoformat(value)
    except ValueError as error:
        raise ValueError(f"{field} 必须是 yyyy-MM-dd") from error


def build_config(args: argparse.Namespace) -> dict[str, object]:
    creation_start = parse_date(args.creation_start, "creation-start")
    creation_end = parse_date(args.creation_end, "creation-end")
    if creation_start > creation_end:
        raise ValueError("创作开始时间不能晚于完成时间")
    if args.published and (not args.publish_date or not args.publish_area):
        raise ValueError("已发表作品必须提供 publish-date 和 publish-area")
    if args.publish_date:
        parse_date(args.publish_date, "publish-date")
    return {
        "action": args.action,
        "title": args.title.strip(),
        "creationStart": args.creation_start,
        "creationEnd": args.creation_end,
        "creationArea": args.creation_area.strip(),
        "rightsBelong": args.rights_belong,
        "published": args.published,
        "publishDate": args.publish_date,
        "publishArea": args.publish_area.strip() if args.publish_area else None,
    }


def validate_config(config: dict[str, object]) -> None:
    required = ("title", "creationArea")
    if any(not config[field] for field in required):
        raise ValueError("作品名称和创作地点不能为空")


def main() -> int:
    args = parse_args()
    try:
        config = build_config(args)
        validate_config(config)
        script = JS_TEMPLATE.replace(
            "__CONFIG__", json.dumps(config, ensure_ascii=False, separators=(",", ":"))
        )
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(script, encoding="utf-8")
    except (OSError, ValueError) as error:
        print(json.dumps({"ok": False, "error": str(error)}, ensure_ascii=False))
        return 1
    print(json.dumps({"ok": True, "output": str(args.output)}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
