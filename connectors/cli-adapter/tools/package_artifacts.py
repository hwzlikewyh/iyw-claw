"""构建可供现有连接器制品接口上传的 ZIP 和摘要清单，不执行发布。"""
import argparse
import hashlib
import json
import zipfile
from pathlib import Path

MAX_ENTRIES = 2048


def package(recipe_path, output):
    recipe = json.loads(recipe_path.read_text(encoding="utf-8"))
    root = recipe_path.parent.parent / "dist" / recipe["id"]
    names = ["adapter.mjs", "connector.json", "install-option.json"]
    names += [item["path"] for item in recipe.get("resources", [])]
    if len(names) != len(set(names)):
        raise ValueError(f'{recipe["id"]} 的制品路径重复')
    for item in recipe.get("resources", []):
        path = (root / item["path"]).resolve()
        if not path.is_relative_to(root.resolve()):
            raise ValueError("制品路径越界")
        if hashlib.sha256(path.read_bytes()).hexdigest() != item["sha256"]:
            raise ValueError(f'{recipe["id"]} 的制品资源摘要不匹配')
    expanded = sum((root / name).stat().st_size for name in names)
    archive = output / (recipe["id"] + "-windows-x86_64.zip")
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as zipped:
        for name in sorted(names):
            zipped.write(root / name, name)
    with zipfile.ZipFile(archive) as zipped:
        if zipped.testzip():
            raise ValueError(f'{recipe["id"]} 的 ZIP CRC 校验失败')
    blockers = []
    if len(names) > MAX_ENTRIES:
        blockers.append("文件数量超过宿主限制")
    return {"id": recipe["id"], "filename": archive.name,
            "sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
            "size": archive.stat().st_size, "expandedSize": expanded,
            "entries": len(names), "blockers": blockers}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    recipes = Path(__file__).resolve().parent.parent / "recipes"
    rows = [package(path, args.output) for path in sorted(recipes.glob("*.json"))]
    (args.output / "artifacts.json").write_text(
        json.dumps(rows, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"packaged": len(rows), "blocked": [r for r in rows if r["blockers"]]},
                     ensure_ascii=False))
