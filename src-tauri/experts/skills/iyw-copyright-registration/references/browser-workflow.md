# 浏览器执行流程

## 目录

- [准备环境](#准备环境)
- [确认会话](#确认会话)
- [上传附件](#上传附件)
- [预检并提交](#预检并提交)
- [批次收尾](#批次收尾)

## 准备环境

将 `$skillDir` 指向当前 Skill 的安装目录。IYW Claw 默认位置如下：

```powershell
$skillDir = Join-Path $env:USERPROFILE ".iyw-claw\skills\iyw-copyright-registration"
$uploadBuilder = Join-Path $skillDir "scripts\build_upload_js.py"
$submissionBuilder = Join-Path $skillDir "scripts\build_submission_js.py"
agent-browser --version
python -c "import openpyxl"
```

读取账号表时使用 `openpyxl.load_workbook(..., read_only=True, data_only=True)`，按表头定位
“客户/账号/密码”列。只在内存中匹配当前客户，不要输出整行、密码或工作簿内容。优先复用
浏览器现有会话；登录页出现时，让当前操作人直接在浏览器中完成登录。

为临时 JavaScript 创建一个明确的临时目录，批次结束后删除其中生成的文件：

```powershell
$runDir = Join-Path ([System.IO.Path]::GetTempPath()) `
  ("iyw-copyright-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $runDir | Out-Null
```

## 确认会话

```powershell
agent-browser open "https://i.iyw.cn/#/IntellectualProperty/CopyrightManage" --json
agent-browser wait 2000 --json
agent-browser snapshot --json
```

从快照确认客户公司名和“剩余版权登记件数”。不一致时停止，不要切换或代猜账号。确认后
打开登记页：

```powershell
agent-browser open "https://i.iyw.cn/#/IntellectualProperty/CopyrightRegister" --json
agent-browser wait 2000 --json
agent-browser snapshot --json
```

## 上传附件

作品上传组件是隐藏的 `input.fileInput`，普通 `agent-browser upload` 可能不触发 Vue 状态。
为每个作品文件单独生成和执行脚本，并等待 OSS 上传完成：

```powershell
$js = Join-Path $runDir "upload-work.js"
python $uploadBuilder --kind work --file "C:\资料\作品一.jpg" --output $js
Get-Content -Raw $js | agent-browser eval --stdin --json
agent-browser wait 4000 --json
```

多图证书按同一方式逐张执行，禁止把多个大文件拼进一次 eval。每次执行后通过页面文件名、
进度 `100%` 或组件文件数确认成功，再继续下一张。

保证书的 drop 处理器绑定在“请上传权利保证书”区域的 `.getImgContainer` 子元素。必须使用
`guarantee` 类型，不能向 `input.el-upload__input` 注入，否则可能进入“其他附件”：

```powershell
$js = Join-Path $runDir "upload-guarantee.js"
python $uploadBuilder --kind guarantee --file "C:\资料\权利保证书\客户保证书.jpg" --output $js
Get-Content -Raw $js | agent-browser eval --stdin --json
agent-browser wait 5000 --json
agent-browser snapshot --json
```

页面应显示保证书图片和“重新上传”。已发表作品的凭证上传区此时可能尚未渲染；先按下一
节运行一次带 `--published` 的 inspect 展开发表字段，再回来上传凭证。

## 预检并提交

以下示例是未发表、法人作品。所有值都必须来自登记清单：

```powershell
$inspectJs = Join-Path $runDir "inspect.js"
python $submissionBuilder `
  --action inspect `
  --title "作品名称" `
  --creation-start "2026-08-06" `
  --creation-end "2026-08-06" `
  --creation-area "中国/浙江/金华" `
  --rights-belong 3 `
  --output $inspectJs
Get-Content -Raw $inspectJs | agent-browser eval --stdin --json
```

未发表作品只接受脚本返回的内层结果 `ok: true`，并核对 `workFileLen`、`showFileLen`、发表
状态、作品名和保证书状态。已发表作品第一次运行时追加：

```powershell
--published --publish-date "2026-08-12" --publish-area "中国/浙江/金华"
```

第一次结果应仅因 `missing publication proof` 未通过；它会把发表状态设为已发表并展开上传
区域。等待页面渲染后上传发表凭证：

```powershell
agent-browser wait 1000 --json
$js = Join-Path $runDir "upload-publish.js"
python $uploadBuilder --kind publish --file "C:\资料\发表证明图片\凭证.jpg" --output $js
Get-Content -Raw $js | agent-browser eval --stdin --json
agent-browser wait 4000 --json
```

使用完全相同的 inspect 参数再运行一次，此时必须返回 `ok: true`。如果第一次 inspect 还
报告其他错误，先解决其他错误，不能把它当作单纯的字段展开步骤。

提交时复用完全相同的业务参数，只把 `--action inspect` 改为 `--action submit` 并生成新文件：

```powershell
$submitJs = Join-Path $runDir "submit.js"
# 重复上一条生成命令的业务参数，仅改 action 和 output。
Get-Content -Raw $submitJs | agent-browser eval --stdin --json
agent-browser wait 5000 --json
agent-browser snapshot --json
```

必须看到“提交成功 我们将在1-3个工作日完成平台审核”才记为成功。页面无响应、网络中断或
返回不确定时不要再次执行提交脚本，先到管理页查重。

## 批次收尾

成功页点击“继续申请版权”，等待登记页加载后再处理下一件：

```powershell
agent-browser find text "继续申请版权" click --json
agent-browser wait 3000 --json
```

批次结束后打开管理页，逐项核验并截图：

```powershell
agent-browser open "https://i.iyw.cn/#/IntellectualProperty/CopyrightManage" --json
agent-browser wait 3000 --json
agent-browser screenshot "版权登记提交结果.png" --json
agent-browser close --json
Remove-Item -LiteralPath $runDir -Recurse -Force
```

只删除本次创建且已确认位于系统临时目录下的 `$runDir`。
