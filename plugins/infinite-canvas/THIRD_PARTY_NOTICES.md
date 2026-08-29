# Third-party notices

## basketikun/infinite-canvas

本插件包含从 `basketikun/infinite-canvas` 固定提交
`ed013e8e5ce8ccab47cf2fc779f8e94555eb4c23` 派生的画布源码。上游根目录以 MIT License
发布，完整文本保存在本目录的 `LICENSE`。

上游 `.codex-plugin/plugin.json` 曾声明 `AGPL-3.0`，但该文件与上游 Codex 插件入口没有
进入本插件源码或制品。本插件发布前仍必须以生产依赖清单和逐文件来源审计为准，不能用
根许可证覆盖未知或不兼容依赖。

## Production dependencies

生产依赖的 package、精确版本、完整性摘要和许可证由以下命令生成并写入发布报告：

```powershell
pnpm licenses list --prod --json
```

AGPL、GPL、SSPL、BUSL 或未知许可证会使制品验证失败。开发依赖不会进入运行时制品，
但仍保留在锁文件中以确保构建可复现。
