# Task 04：系统 Skill 发布源与镜像输入

## 目标

将 `skill` 仓库收敛为可审计的系统 Skill 发布源，提供后端可消费的稳定清单和制品输入；不再让桌面端直接 clone/pull/reset 该仓库。

## scope_write

- `F:\projects\iyw\skill\AGENT.md`（如不存在则新增项目规则）
- 仓库根 `experts.toml`
- 本仓库的发布校验脚本、清单生成脚本和 CI
- 与发布格式直接相关的文档

## 禁止修改

- Skill 业务内容，除非为修复清单一致性且在 handoff 中逐项列出。
- `iyw-claw`、`iyw-fusion-api` 代码。
- 已发布稳定标签；禁止移动、删除或重签旧标签。

## 发布契约

每个稳定标签必须满足：

- `experts.toml bundle.version` 与 `v<semver>` 标签一致。
- 每个 expert ID 唯一、目录存在、包含 `SKILL.md`。
- 依赖只引用已声明 ID，无重复、自依赖和环。
- 路径大小写、跨平台文件名和相对路径合法。
- 不含 `.git`、缓存、构建产物、临时文件、凭据、绝对路径或本机配置。
- 生成 `release-manifest.json`：schema、bundle version、commit、每文件 size/sha256、Skill metadata、依赖图和总 raw size。
- manifest 使用稳定排序和规范 JSON，便于后端复验。

## 后端拉取边界

本任务只定义后端 handler 的输入，不实现后端：

- 后端只接受编译配置中的 repository identity 和稳定标签，不执行数据库传入的任意 Git URL/命令。
- 使用部署凭据/CI deploy token，不把凭据放进仓库、URL、日志或客户端。
- clone/fetch 到隔离临时目录，checkout detached tag；验证 tag/commit 和 manifest。
- 后端再按每个 Skill 构建不可变 artifact 上传 TOS。

## 实施步骤

1. 清点当前 `experts.toml` 与目录，输出缺失、重复、孤儿、环和未跟踪发布文件。
2. 增加纯校验脚本，退出码非零阻止发布。
3. 增加 manifest 生成和二次复验命令；生成物在 release pipeline 产生，不污染开发工作区。
4. CI 对 push/merge 做校验，对稳定标签做完整 release gate。
5. 文档化 tag 创建、签名、撤销（只能发布新修复版本）和后端镜像操作。
6. 扫描当前和 Git 历史的凭据；发现泄露立即记录轮换项，不在报告中回显值。

## 验证

- 对当前树运行 manifest 校验两次，输出字节完全一致。
- 人为构造缺 SKILL.md、循环依赖、重复 ID、标签版本不一致、危险路径，均被拒绝。
- 在 Windows/Linux CI 验证路径一致性。
- 生成 release manifest 后逐文件重新 SHA-256，全部匹配。

## 完成定义

- `skill` 成为唯一发布源且有机器可验证契约。
- 桌面无需 Git 凭据即可消费后端发布目录。
- 所有已知凭据风险已轮换或形成 P0 阻塞，不把密钥写进 handoff。
