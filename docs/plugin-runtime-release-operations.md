# 插件运行时发布密钥操作

v2 本地可执行插件使用独立 Minisign 密钥，不复用 Tauri App、Agent 或 Toolchain 密钥。

## Fusion 配置

Fusion 不要求插件签名密钥。v2 Plugin artifact 依靠 TOS 对象大小、SHA-256 和 manifest/
组件契约校验；`signature` 与 `signatureKeyId` 字段保留为空以兼容旧数据。

## Claw 构建配置

正式 Tauri release 和 self-hosted release test 不需要插件公钥变量。

## 验证

- TOS ZIP 大小、SHA-256 或 manifest/组件任一项不匹配时，Claw 在安装前拒绝；
- v2 artifact 的 install-plan 包含精确 artifactSize 和 SHA-256；
- v1 插件及普通 Skill 不要求插件签名；
- 日志只记录 key id 和校验阶段，不记录私钥、密码或完整签名。
