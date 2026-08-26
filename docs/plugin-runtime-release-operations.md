# 插件运行时发布密钥操作

v2 本地可执行插件使用独立 Minisign 密钥，不复用 Tauri App、Agent 或 Toolchain 密钥。

## Fusion 配置

Fusion 制品构建节点必须配置：

```text
PLUGIN_RELEASE_SIGNING_KEY=<base64 or minisign secret key text>
PLUGIN_RELEASE_SIGNING_PASSWORD=<secret key password>
PLUGIN_RELEASE_SIGNING_KEY_ID=<stable public key id>
```

私钥和密码只保存在发布密钥系统。`PLUGIN_RELEASE_SIGNING_KEY_ID` 是可公开的稳定标识，
建议使用 `plugin-release-YYYY-NN`。签名器或 key id 缺失时，Skill、Expert 和 v1 Plugin
仍可构建；v2 Plugin artifact 必须进入 `plugin_artifact_signing_failed`，不得 ready。

## Claw 构建配置

GitHub Repository Variables 必须配置：

```text
IYW_PLUGIN_RELEASE_PUBLIC_KEY=<matching minisign public key>
IYW_PLUGIN_RELEASE_PUBLIC_KEY_ID=<same stable key id>
```

正式 Tauri release 和 self-hosted release test 在 Rust 编译时读取这两个值；缺失时工作流
直接失败。公钥可以公开，但不得从 Fusion install-plan 动态下发并直接信任。

## 轮换

首版每个 Claw build 只信任一个插件公钥。轮换顺序固定为：

1. 生成新的独立密钥和 key id；
2. 先发布包含新公钥的 Claw；
3. 确认最低受支持 Claw 已覆盖新公钥；
4. 再将 Fusion 私钥和 key id 切换到新值；
5. 旧 Claw 对新签名明确返回不受信 key id，不降级到仅 SHA-256。

需要同时接受新旧两把密钥时，先扩展客户端为编译期固定 keyring，并单独评审；不得把远端
返回的公钥加入信任链。

## 验证

- 篡改 ZIP、签名或 key id 任一项，Claw 均在解压前拒绝；
- v2 artifact 的 install-plan 同时包含精确 artifactSize、SHA-256、signature 和 signatureKeyId；
- v1 插件及普通 Skill 不要求插件签名；
- 日志只记录 key id 和校验阶段，不记录私钥、密码或完整签名。
