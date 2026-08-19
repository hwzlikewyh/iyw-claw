---
name: iyw-copyright-registration
description: 在爱原物 IYW（i.iyw.cn）为客户准备、检查并批量提交美术作品版权登记，支持账号会话确认、作品与权利保证书上传、未发表或已发表资料填写、单证多图、提交结果核验和失败排查。用户提到 iyw 版权登记、爱原物版权、批量提交版权、上传权利保证书、版权登记自动化或在 IYW 后台提交美术作品登记时使用。
routing:
  capability: IYW 美术作品版权登记
  coreTriggers: [提交或检查 IYW 版权登记]
  exclusions: [通用版权法律咨询]
  aliases: [爱原物版权, 批量版权登记]
  invocation: 先读 SKILL.md 并核对批次资料，再按文档操作后台。
---

# IYW 版权登记

通过 `agent-browser` 操作 IYW 版权登记后台。先生成本批登记清单并逐项检查，再执行会
消耗登记件数且提交后不可修改的操作。不要猜测页面结构、客户身份或登记字段。

## 执行边界

- 只处理用户明确指定的客户、作品文件和登记批次。
- 把提交审核视为外部写操作。当前请求已明确要求“提交”时可在预检通过后执行；只要求
  整理、检查或演示时停在预检，不调用 `submit(1)`。
- 提交结果不确定时不要重试。先到版权管理页按客户和作品名核对，避免重复扣件。
- 不输出账号、密码、Cookie、完整 base64、OSS 签名参数或保证书 URL。
- 页面选择器、Vue 内部字段或成功文案与参考不符时立即停止并报告，不自行猜新字段。

## 前置检查

1. 确认 `agent-browser` 可用，并准备 Python 3.10+；读取 `.xlsx` 时确认可导入
   `openpyxl`。
2. 找到账号表和客户目录，但不要打印密码。账号表预期包含“客户/账号/密码”列。
3. 为每个证书建立登记清单：客户、作品名、作品文件组、发表状态、创作起止时间、创作
   地点、权利归属、保证书，以及已发表作品的发表时间、地点和凭证。
4. 一张证书包含几张图不明确时先询问。不要把同目录图片静默合并；单证最多六张。
5. 检查作品名、文件组和登记字段重复项。提交前记录管理页剩余登记件数和当前客户名称。

字段值、文件限制和变体要求见 [references/fields-and-variants.md](references/fields-and-variants.md)。

## 标准流程

1. 按 [references/browser-workflow.md](references/browser-workflow.md) 打开管理页并确认登录
   客户与清单一致。会话失效时让当前操作人完成登录，不在终端回显凭据。
2. 打开登记页，先上传本证书的全部作品文件，每个文件上传完成后再传下一个。
3. 上传本次登记的权利保证书。每件登记都重新上传，不复用上一件的 OSS URL。
4. 使用内置提交脚本的 `inspect` 模式设置字段并读取脱敏检查结果。已发表作品第一次
   inspect 会展开发表字段，随后上传发表凭证并重新 inspect；未发表作品不得填写发表字段。
5. 只有所有必填字段、作品文件和展示图数量均符合清单时才能继续。
6. 用户已授权提交且检查通过后，使用同一参数生成 `submit` 模式脚本并执行一次。
7. 等待页面明确显示提交成功，再点击“继续申请版权”处理下一件。失败时保留页面状态，
   按 [references/troubleshooting.md](references/troubleshooting.md) 排查。
8. 全批次结束后回到管理页，逐项核对作品名和状态，保存结果截图并关闭浏览器。

## 结果报告

报告客户、计划件数、成功件数、未提交或结果不确定的作品名、管理页核验结果和剩余登记
件数。不要把凭据、内部 Vue 对象或附件 URL 带入报告。

## 资源导航

- [references/browser-workflow.md](references/browser-workflow.md)：完整命令、上传脚本与提交步骤。
- [references/fields-and-variants.md](references/fields-and-variants.md)：字段合同、已发表作品和单证多图。
- [references/troubleshooting.md](references/troubleshooting.md)：已知页面问题、诊断顺序和停止条件。
- `scripts/build_upload_js.py`：安全生成作品、保证书或发表凭证的浏览器注入脚本。
- `scripts/build_submission_js.py`：生成字段预检或单次提交脚本。
