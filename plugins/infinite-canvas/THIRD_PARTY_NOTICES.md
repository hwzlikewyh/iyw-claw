# Third-party notices

## basketikun/infinite-canvas

本插件包含从 `basketikun/infinite-canvas` 固定提交
`9cd3deacbcc598361807afc16c059a80dd18b6f9` 派生的画布源码。上游根目录以 MIT License
发布，完整文本保存在本目录的 `LICENSE`。

上游 `.codex-plugin/plugin.json` 曾声明 `AGPL-3.0`，但该文件与上游 Codex 插件入口没有
进入本插件源码或制品。本插件发布前仍必须以生产依赖清单和逐文件来源审计为准，不能用
根许可证覆盖未知或不兼容依赖。

## Provenance audit (2026-11-12)

`upstream.json` 固定了上游 commit 和 MIT 根许可证；构建 verifier 会拒绝缺失或非 MIT 的
上游 provenance，并逐项拒绝 `.codex-plugin`、`.claude-plugin` 和 `.mcp.json`。打包清单只
包含本插件的 runtime、Widget、contracts、Skills、许可证和 notices，因此错误的 AGPL 元数据
文件不在源码构建输入或发布 artifact 中。生产依赖仍由 verifier 单独按 package license
审计，任何 AGPL/GPL/SSPL/BUSL/UNKNOWN 都会 fail closed。

## Production dependencies

生产依赖包名、精确版本和许可证由 verifier 根据已解析的 package metadata 生成并写入
`dist/license-report.json`；verifier 不生成 tarball 完整性摘要。生产依赖包含
CC-BY-4.0（`caniuse-lite` 1.0.30001826，Ben Briggs，数据源来自
[caniuse.com](https://caniuse.com)，完整 Attribution 4.0 文本见该包的 `LICENSE`）、
CC0-1.0（`highlightjs-vue`）、
BlueOak-1.0.0（minimatch）、0BSD（tslib）及 MIT/CC0 双许可（type-fest）。MIT、ISC、
BSD-2-Clause、BSD-3-Clause、Apache-2.0 及上述明确许可证会写入制品许可证报告；
AGPL、GPL、SSPL、BUSL 或未知许可证会使验证失败。开发依赖不会进入运行时制品，
但仍保留在锁文件中以确保构建可复现。
