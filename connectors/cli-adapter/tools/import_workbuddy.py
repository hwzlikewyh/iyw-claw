import argparse
import json
from pathlib import Path

from import_support import (add_package, command, load, parse_command, public_env,
                            selected, tool_for, words)
import import_overrides

CUSTOM_INSTALL = {"77ircloud", "meituan-for-business", "wps-knowledgebase", "tc-chengxin",
                  "mglc", "uupt", "woscli", "shanlong-claw", "dingtalk", "cloudhub"}
CUSTOM_COMMAND = {"woscli", "uupt", "shanlong-claw", "zenava"}


def portable_resources(recipe, folder, assets):
    roots = {"assets": assets.resolve(), "workbuddy": folder.parent.resolve()}
    for item in recipe["resources"]:
        path = Path(item["source"]).resolve()
        for name, root in roots.items():
            if path.is_relative_to(root):
                item["source"] = name + "/" + path.relative_to(root).as_posix()
                break
        else:
            raise ValueError("构建资源不在供应商或制品目录内")


def installation(recipe, source, metadata):
    name = recipe["id"]
    if name in CUSTOM_INSTALL:
        return
    if source.get("runtime", {}).get("type") == "python":
        recipe["python"] = ["tccli==3.1.209.1"] if name == "emr-query" else ["zenava==0.5.8", "tzdata==2026.6"]
        return
    text = selected(source.get("init", {}))
    if not text:
        return
    for part in text.split("&&"):
        part = part.strip()
        if part.startswith('set "NODE_OPTIONS='):
            recipe["env"]["NODE_OPTIONS"] = ""
            continue
        tokens = words(part)
        if tokens[0].lower() in ("npm", "npm.cmd") and tokens[1] in ("i", "install"):
            for package in tokens[2:]:
                if not package.startswith("-"):
                    add_package(recipe, package, metadata)
        else:
            recipe["install"].append(parse_command(part, recipe, metadata))


def authentication(recipe, source, metadata):
    if recipe["id"] in CUSTOM_COMMAND:
        recipe["login"] = [{"command": command("custom")}]
        recipe["status"] = command("custom")
        recipe["logout"] = [command("custom")]
        return
    steps = source["auth"] if isinstance(source["auth"], list) else [source["auth"]]
    for step in steps:
        value = {"command": parse_command(selected(step), recipe, metadata)}
        if isinstance(step, dict) and step.get("skipIf"):
            value["skipIf"] = parse_command(selected(step["skipIf"]), recipe, metadata)
        recipe["login"].append(value)
    recipe["status"] = parse_command(selected(source["status"]), recipe, metadata)
    if source.get("unAuth"):
        recipe["logout"] = [parse_command(selected(source["unAuth"]), recipe, metadata)]


def mcp_entry(recipe, folder, metadata):
    path = folder / "mcp.json"
    if not path.is_file():
        return
    raw = next(iter(load(path)["mcpServers"].values()))
    if folder.name == "welife-ai-workbench-connector":
        return
    program = raw["command"]
    args = raw.get("args", [])
    if program == "npx":
        recipe["mcp"] = parse_command(" ".join([program, *args]), recipe, metadata)
    else:
        recipe["mcp"] = command(program, args)
    recipe["env"].update(raw.get("staticEnv", {}))


def convert(folder, inputs):
    source = import_overrides.source_override(folder, load(folder / "cli.json"))
    recipe = {"id": folder.name, "name": inputs["names"].get(folder.name, folder.name),
              "env": public_env(folder, source.get("env", {})), "packages": [], "python": [],
              "install": [], "login": [], "logout": [], "tools": [], "resources": []}
    for key in ("statusMatch", "statusMatchJson"):
        if source.get(key):
            recipe[key] = source[key]
    installation(recipe, source, inputs["metadata"])
    authentication(recipe, source, inputs["metadata"])
    recipe["tools"] = [tool_for(recipe["login"])]
    if source.get("versionCheck"):
        version = source["versionCheck"]
        recipe["versionCheck"] = {"minimum": version["minVersion"], "pattern": version.get("versionPattern")}
        if folder.name not in CUSTOM_COMMAND:
            recipe["versionCheck"]["command"] = parse_command(selected(version["command"]), recipe, inputs["metadata"])
    mcp_entry(recipe, folder, inputs["metadata"])
    import_overrides.apply(recipe, folder, inputs["assets"])
    portable_resources(recipe, folder, inputs["assets"])
    if recipe.get("mcp"):
        recipe["tools"] = []
    return recipe


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--metadata", type=Path, required=True)
    parser.add_argument("--assets", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    catalog = load(args.source / ".codebuddy-connector/connectors.json")
    inputs = {"metadata": args.metadata, "assets": args.assets,
              "names": {item["id"]: item.get("name", item["id"]) for item in catalog["connectors"]}}
    args.output.mkdir(parents=True, exist_ok=True)
    results = []
    for folder in sorted((args.source / "connectors").iterdir()):
        if not (folder / "cli.json").is_file():
            continue
        try:
            recipe = convert(folder, inputs)
            (args.output / f"{folder.name}.json").write_text(
                json.dumps(recipe, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
            results.append({"id": folder.name, "converted": True})
        except (ValueError, KeyError, FileNotFoundError) as error:
            results.append({"id": folder.name, "converted": False, "reason": str(error)})
    print(json.dumps(results, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
