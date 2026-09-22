# Agent 提问表单

`ask_user_question` 使用现有会话问答通道等待回答。一次可包含 1 至 32 个相关字段，返回格式仍为 `answers[].selected: string[]`。

## 布局与控件

- `layout: "form"`：一页填写多个字段，支持分组与复核。
- `layout: "steps"`：逐题回答。用户可以在两种布局间切换，已填内容保留。
- 未指定布局的旧请求继续使用逐题模式；带 `input` 或 `ui` 的请求默认表单模式。
- `ui.control` 支持 `text`、`textarea`、`radio`、`checkbox`、`select`、`combobox`、`number`、`date`、`switch`、`password`、`repeat`。
- `combobox` 提供搜索、键盘选择与空结果状态；多项选择使用 `checkbox`。
- `optional` 标识选填；`ui.group` 对相邻字段分组；`ui.placeholder` 设置输入提示。
- `input` 是 JSON Schema 属性，支持类型、长度、数值范围、格式和默认值。秘密字段不支持默认值。
- 日期字段可用 `ui.not_before` 引用之前的日期字段，限制结束日期不早于开始日期。

旧版 `question/header/multiSelect/options` 请求保持兼容。旧请求继续允许自定义回答；显式 `select/combobox` 使用固定选项。推荐标签不会预先选中。

## 选项中的下拉

字段可指定请求内唯一 `id`。`ui.when.question_id` 引用之前的字段，`equals` 比较完整选项标签或文本值，开关使用 `"true"` / `"false"`。只能向前引用，不能引用密码或重复条目字段。

当条件父字段是单选或多选时，子字段展示在对应选项中。其他条件字段在表单中按顺序展开。隐藏的字段保留本地草稿，但不参与校验、不发送给 Agent。

```json
{
  "layout": "form",
  "questions": [
    {
      "id": "output",
      "question": "输出形式",
      "header": "输出形式",
      "options": [{ "label": "文档" }, { "label": "表格" }],
      "ui": { "control": "radio", "group": "交付要求" }
    },
    {
      "id": "format",
      "question": "文档格式",
      "header": "文档格式",
      "options": [{ "label": "Markdown" }, { "label": "Word" }, { "label": "PDF" }],
      "ui": {
        "control": "select",
        "when": { "question_id": "output", "equals": "文档" }
      }
    },
    {
      "id": "audience",
      "question": "汇报对象",
      "header": "汇报对象",
      "options": [{ "label": "项目团队" }, { "label": "管理层" }, { "label": "客户" }],
      "ui": { "control": "combobox" }
    },
    {
      "id": "notes",
      "question": "补充要求",
      "optional": true,
      "ui": { "control": "textarea", "placeholder": "需要重点关注的内容" },
      "input": { "type": "string", "maxLength": 500 }
    }
  ]
}
```

## 重复条目

```json
{
  "question": "重点项目",
  "optional": true,
  "ui": {
    "control": "repeat",
    "max_rows": 5,
    "columns": [
      { "id": "name", "label": "项目名称", "required": true },
      { "id": "focus", "label": "重点关注", "placeholder": "选填" }
    ]
  }
}
```

重复条目支持最多 20 行、每行 1 至 6 列。单个字段答案仍限制为 4096 个字符，整个数组序列化后计入此限制。全空行在提交时忽略，有内容的行校验其必填列。

结果示例：`selected: ["[{\"name\":\"iyw-claw\",\"focus\":\"发布进度\"}]"]`。按 JSON 解析这一个字符串即可取得结构化行数据，不要按逗号拆分。

## 生命周期

“稍后填写”只折叠当前问答，不向 Agent 发送拒绝，Agent 仍等待回答；“跳过”才会发送 `declined: true`。草稿仅存在当前组件内存，刷新页面或卸载会话后不保证保留。

提交期间禁用输入和重复提交；失败保留答案并可重试。后端重新验证可见字段、答案数量、类型与重复条目。旧会话和原生 ACP 表单继续使用现有回传通道。

密码输入默认遮挡，可临时查看；复核和历史卡片不展示明文。密码答案仍会交给发起提问的 Agent，因此这不是凭证保险库，不用于长期保存凭证。
