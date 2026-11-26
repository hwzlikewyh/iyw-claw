import hashlib
import json
import re
from pathlib import Path


def load(path):
    return json.loads(Path(path).read_text(encoding="utf-8-sig"))


def words(text):
    result, word, quote = [], [], None
    for char in text.strip():
        if quote:
            if char == quote:
                quote = None
            else:
                word.append(char)
        elif char in "\"'":
            quote = char
        elif char.isspace():
            if word:
                result.append("".join(word))
                word = []
        else:
            word.append(char)
    if quote:
        raise ValueError("命令引号不完整")
    if word:
        result.append("".join(word))
    if any(value in ("&", "&&", "||", ";") for value in result):
        raise ValueError("包含尚未转换的 shell 控制流")
    return result


def selected(value):
    if isinstance(value, str):
        return value
    if not isinstance(value, dict):
        raise ValueError("缺少 Windows 命令")
    if "command" in value:
        return selected(value["command"])
    return value.get("win32") or ""


def command(program, args=None):
    return {"program": program, "args": args or []}


def public_env(folder, source):
    result = {}
    for key, value in source.items():
        value = value.replace("${HOME}", "$HOME")
        vendor = f"$HOME/.workbuddy/connectors-marketplace/connectors/{folder.name}"
        result[key] = value.replace(vendor, "{{ARTIFACT_DIR}}/vendor").replace("$HOME", "{{AUTH_DIR}}")
    return result


def expand(text, env):
    values = {"USERPROFILE": "{{AUTH_DIR}}", "HOME": "{{AUTH_DIR}}", **env}
    for _ in range(len(values) + 1):
        replaced = re.sub(r"%([A-Za-z_][A-Za-z0-9_]*)%", lambda m: values.get(m[1], m[0]), text)
        if replaced == text:
            break
        text = replaced
    return text.replace("\\", "/") if "{{" in text else text


def package_metadata(spec, metadata_root):
    filename = re.sub(r"[^a-zA-Z0-9._-]", "_", spec) + ".json"
    data = load(metadata_root / filename)
    if data.get("error") or not data.get("version"):
        raise ValueError(f"缺少官方包信息：{spec}")
    return data


def add_package(recipe, spec, metadata_root):
    data = package_metadata(spec, metadata_root)
    dependency = {"name": data["name"], "spec": f'{data["name"]}@{data["version"]}',
                  "integrity": data.get("dist", {}).get("integrity", "")}
    if not any(item["name"] == dependency["name"] for item in recipe["packages"]):
        recipe["packages"].append(dependency)
    return data


def parse_command(text, recipe, metadata_root):
    prefix = 'set "NODE_OPTIONS=" && '
    if text.startswith(prefix):
        recipe["env"]["NODE_OPTIONS"] = ""
        text = text[len(prefix):]
    tokens = words(text)
    if not tokens:
        raise ValueError("命令为空")
    program, args = tokens[0], tokens[1:]
    if program.lower() in ("npx", "npx.cmd"):
        while args and args[0] in ("-y", "--yes"):
            args.pop(0)
        package = add_package(recipe, args.pop(0), metadata_root)
        binaries = package.get("bin") or {}
        if not isinstance(binaries, dict) or not binaries:
            raise ValueError(f'npx 包没有 bin：{package["name"]}')
        program = package["name"].split("/")[-1]
        if program not in binaries:
            program = next(iter(binaries))
        return {**command(program, args), "package": package["name"]}
    return command(expand(program.removesuffix(".cmd"), recipe["env"]),
                   [expand(arg, recipe["env"]) for arg in args])


def resource(path, destination):
    return {"source": str(path), "path": destination,
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def tool_for(login):
    entry = dict(login[0]["command"])
    args = entry["args"]
    cut = next((i for i, value in enumerate(args)
                if value in ("auth", "login", "config") or ".auth." in value), len(args))
    entry["args"] = args[:cut]
    return {"name": "connector_cli", "description": "使用服务方官方命令行能力", "command": entry}
