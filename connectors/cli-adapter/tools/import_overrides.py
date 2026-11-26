import tarfile

from import_support import command, load, resource


def vendor_files(recipe, folder, patterns):
    for pattern in patterns:
        for path in folder.glob(pattern):
            if path.is_file() and not path.name.startswith("test"):
                recipe["resources"].append(resource(path, "vendor/" + path.relative_to(folder).as_posix()))


def bundled_package(recipe, folder, package_path):
    with tarfile.open(package_path, "r:gz") as archive:
        metadata = load_archive_json(archive, "package/package.json")
    destination = "vendor/cli-package.tgz"
    recipe["resources"].append(resource(package_path, destination))
    recipe["packages"].append({"name": metadata["name"], "spec": "{{ARTIFACT_DIR}}/" + destination})


def load_archive_json(archive, name):
    import json
    return json.load(archive.extractfile(name))


def source_override(folder, source):
    """将供应商已有脚本的固定控制流转为结构化步骤。"""
    if folder.name == "feishu":
        source["auth"][1]["command"]["win32"] = "lark-cli auth login --recommend"
    if folder.name == "gaoding":
        source["status"]["win32"] = "gd-cli auth status --json"
        source["statusMatchJson"] = {"logged_in": True, "credential.valid": True}
        source.pop("statusMatch", None)
    if folder.name == "emr-query":
        source["status"]["win32"] = ("tccli emr DescribeInstancesList --region ap-guangzhou "
            "--version 2019-01-03 --cli-unfold-argument --DisplayStrategy clusterList --Limit 1")
        source.pop("statusMatch", None)
    return source


def auth_overrides(recipe):
    name = recipe["id"]
    if name == "feishu":
        recipe["login"][1]["successCodes"] = [0, 3]
        recipe["env"]["LARKSUITE_CLI_CONFIG_DIR"] = "{{AUTH_DIR}}/.lark-cli"
        recipe["status"] = command("lark-cli", ["auth", "status", "--json"])
        recipe["statusJsonRoot"] = "data"
        recipe["statusRejectJson"] = {"verified": False}
        recipe["logout"] = [command("lark-cli", ["auth", "logout"])]
    if name == "wecom":
        recipe["env"]["WECOM_CLI_CONFIG_DIR"] = "{{AUTH_DIR}}/.config/wecom"
        recipe["statusMatch"] = r"(?:^|\n)Status: authorized\s*(?:\r?\n|$)"
        recipe["logout"] = []
        recipe["clearAuthDirectories"] = ["{{AUTH_DIR}}/.config/wecom"]


def apply(recipe, folder, assets):
    name = folder.name
    auth_overrides(recipe)
    if name == "dingtalk":
        recipe["env"]["DWS_NO_SKILLS"] = "1"
        for license_name in ("LICENSE", "NOTICE"):
            recipe["resources"].append(resource(assets / name / license_name, license_name))
    if name in ("77ircloud", "meituan-for-business", "wps-knowledgebase"):
        bundled_package(recipe, folder, assets / name / "package.tgz")
    if name == "wps-knowledgebase":
        recipe["configDefaults"] = [{"path": "{{AUTH_DIR}}/.kwiki-cli/config.json", "values": {"source": "workbuddy"}}]
    if name == "tc-chengxin":
        bundled_package(recipe, folder, folder / "cli/tc-chengxin-cli.tgz")
    if name in ("mglc", "uupt", "woscli", "shanlong-claw", "dingtalk", "cloudhub"):
        native(recipe, name, assets)
    if name == "cloudhub":
        commands = [recipe["status"], *recipe["logout"], recipe["tools"][0]["command"],
                    *(step["command"] for step in recipe["login"])]
        for entry in commands:
            entry["args"] = ["--profile", "iyw-{{ACCOUNT_ID}}", *entry["args"]]
    if name == "shanlong-claw":
        vendor_files(recipe, folder, ["skills/*/SKILL.md"])
        recipe["tools"][0]["helpFiles"] = [item["path"] for item in recipe["resources"]
                                            if item["path"].endswith("/SKILL.md")]
        for line in (folder / "default.env").read_text(encoding="utf-8-sig").splitlines():
            if not line.strip() or line.lstrip().startswith("#") or "=" not in line:
                continue
            key, value = line.split("=", 1)
            if value:
                recipe["env"][key] = value
        recipe["install"] = [command(recipe["status"]["program"],
            ["--sl-config-migrate", "--home", "{{AUTH_DIR}}/.slclaw"])]
    if name == "welife-ai-workbench-connector":
        vendor_files(recipe, folder, ["client.cjs"])
        recipe["mcp"] = command("node", ["{{ARTIFACT_DIR}}/vendor/client.cjs", "mcp"])
    if name == "zenava":
        zenava(recipe, folder)
    if name == "seeyon-office-marketing-suite":
        seeyon(recipe, folder)


def native(recipe, name, assets):
    metadata = load(assets / name / "native.json")
    binary = metadata["filename"]
    binary_resource = resource(assets / name / binary, "bin/" + binary)
    runtime_file = {"path": binary_resource["path"], "sha256": binary_resource["sha256"]}
    recipe["resources"].append(binary_resource)
    recipe["runtimeFiles"] = [runtime_file]
    entry = "{{RUNTIME_DIR}}/bin/" + binary
    version = recipe.setdefault("versionCheck", {"minimum": metadata.get("version", "1.0.0")})
    version["command"] = command(entry, version.get("command", {}).get("args", ["--version"]))
    recipe["install"] = []
    for item in recipe["login"]:
        item["command"]["program"] = entry
    recipe["status"]["program"] = entry
    for item in recipe["logout"]:
        item["program"] = entry
    recipe["tools"] = [{"name": "connector_cli", "description": "使用官方 CLI", "command": command(entry)}]
    if name in ("uupt", "woscli"):
        prefix = ["auth"] if name == "uupt" else []
        recipe["login"] = [{"command": command(entry, [*prefix, "login"])}]
        recipe["status"] = command(entry, [*prefix, "status"])
        recipe["logout"] = [command(entry, [*prefix, "logout"])]
    if name == "shanlong-claw":
        recipe["login"] = [{"command": command(entry, ["connector", "auth"])}]
        recipe["status"] = command(entry, ["connector", "status"])
        recipe["logout"] = [command(entry, ["connector", "unauth"])]
        recipe["env"]["SL_CLI_SKIP_UPDATE"] = "1"


def zenava(recipe, folder):
    vendor_files(recipe, folder, ["scripts/auth_server.py", "scripts/assets/*"])
    script = "{{ARTIFACT_DIR}}/vendor/scripts/auth_server.py"
    recipe["login"] = [{"command": command("python", [script])}]
    recipe["status"] = command("python", [script, "--status"])
    recipe["logout"] = [command("python", [script, "--logout"])]
    recipe["tools"] = [{"name": "connector_cli", "description": "使用 Zenava CLI", "command": command("zenava")}]
    recipe["versionCheck"] = {"command": command("zenava", ["--version"]), "minimum": "0.5.8"}


def seeyon(recipe, folder):
    vendor_files(recipe, folder, ["connector-config.json", "cli/seeyon-connector.js",
                                 "skills/*/scripts/*.py"])
    recipe["python"] = ["requests==2.34.2", "pycryptodome==3.24.1"]
    recipe["parameters"] = [{"key": "OA_BASE_URL", "label": "企业 OA 地址", "kind": "string",
        "required": True, "secret": False, "description": "填写本企业致远 OA 的访问地址"}]
    base = "{{ARTIFACT_DIR}}/vendor/skills/seeyon-collaborative-office-loop-skill/scripts/"
    recipe["tools"] = [
        {"name": "seeyon_office", "description": "致远 OA 办公操作，先调用 --help 查看参数",
         "command": command("python", [base + "seeyon_office.py"])},
    ]
