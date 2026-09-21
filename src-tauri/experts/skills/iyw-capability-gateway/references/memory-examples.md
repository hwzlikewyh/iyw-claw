# 记忆操作示例

本页服务于当前宿主公布的 `manage_iyw_memory`。调用其真实命名空间；不要创建同名工具、
猜测 capability ID、直接读写本地记忆文件或把用户路径/令牌放入参数。
宿主返回的实时 schema 与权限优先于示例。常用操作带有内联 schema；维护操作先读取其
已公布的 capability schema，一次会话内复用，不在每次调用前重复查询。

## 操作选择

| 当前情况 | 操作 | 完成证据 |
| --- | --- | --- |
| 依赖以前的约定、偏好、做法 | `recall` | 相关且当前有效的结果，允许无匹配 |
| 用户明确表达长期事实或要求 | `append` | 成功持久化回执 |
| 观察到可能稳定的行为，仍不确定 | `propose` | 候选回执，仍为暂定信息 |
| 明确纠正某条已保存内容 | `documents.correct` | 当前 etag 对应的事务提交成功 |
| 撤销旧约定、已知有效期结束 | `retire` | `excludedFromRecall=true` 或明确将来截止时间 |
| 查看尚待归纳的行为 | `candidates.list` / `candidate.resolve` | 当前 candidate ID 与 revision 下的处置结果 |
| 处理后台提出的过时信息 | `maintenance.read` / `review.resolve` | 对应 review 的 `resolved=true` |
| 排查记忆未生效 | `settings.read` / `harvest.status` | 读取实际状态，区分未采集、待处理与失败 |

以下 `<...>` 只标识需要绑定的工具返回值，绝不原样提交，也不要猜测真实 ID。
所有调用的业务字段放在 `parameters` 内，不再套一层 `request` 或 `arguments`。

## 明确偏好无需再问一次

用户说“以后版本说明写给客户看，不要讲代码细节”，意思和长期范围明确。
已有相关记忆足够时直接复用；怀疑重复时先做一次聚焦 recall。

```json
{"operation":"recall","parameters":{"query":"面向客户的版本说明表达方式","limit":3}}
```

没有相同有效规则时保存简明原意：

```json
{"operation":"append","parameters":{"content":"面向客户的版本说明按用户可感知变化描述，不展开代码实现细节。"}}
```

只有宿主确认写入成功后才说已记住。失败不能以“好的，记住了”代替存储。
用户不必先说“记住”；不要再追加“要不要帮你保存”的常规确认。

## 局部例外保留范围

已知“文档默认 PPT/PDF”，用户新增“周报直接发聊天，不用文件”：保存周报例外，
不能停用适用于其他文档的默认要求。

```json
{"operation":"append","parameters":{"content":"周报直接在聊天中输出，不需要附加文件；其他文档仍按既有交付约定。"}}
```

一次性的“这次直接贴出来”属于当前任务，不升级为长期偏好。
项目/任务边界由宿主上下文提供，Agent 不自行构造用户 ID 或 workspace 选择器。

## 用户纠正后处理旧记录

用户随后明确“以后周报也改成 PDF”。先召回周报旧规则并读取最小必需文档，
取得精确原文和 etag；读取已公布的 `iyw.memory.documents.correct.v1` schema，
用其字段执行 `documents.correct`。这条路径在一个宿主事务中更新旧/新内容和候选引用。
不要用两条不相关的新旧记录长期并存来表示纠正。

```json
{"operation":"documents.read","parameters":{"documents":["memory"]}}
```

`oldContent` 必须是刚读取的精确条目正文，`expectedEtag` 取该文档的当前 etag：

```json
{
  "operation":"documents.correct",
  "parameters":{
    "document":"memory",
    "oldContent":"<刚读取的周报旧规则正文>",
    "newContent":"以后周报以 PDF 文件交付。",
    "expectedEtag":"<memory 文档当前 etag>"
  }
}
```

若用户只要求停止采用某条已召回经验，可使用精确 ID/revision 退役：

```json
{
  "operation":"retire",
  "parameters":{
    "memoryId":"<recall 返回的 id>",
    "expectedRevision":"<同一结果的 sourceRevision>",
    "reason":"用户明确撤销了这条旧约定。"
  }
}
```

回执成功后，当前对话中也停止使用旧结论。退役保留可追溯历史，不表示彻底删除聊天或备份。

## 有证据的到期时间

只有用户或原始来源明确给出时间与时区时才设置 `expiresAt`：

```json
{
  "operation":"retire",
  "parameters":{
    "memoryId":"<已召回条目的 id>",
    "expectedRevision":"<同一条目的 sourceRevision>",
    "reason":"该约定仅在用户明确指定的活动有效期内适用。",
    "expiresAt":"2026-10-01T00:00:00+08:00"
  }
}
```

日期仅为格式示例，必须替换成真实证据中的截止时间。没有年份的节日、不常使用的规则、
长时间未提及的偏好都不能靠猜测设置到期。到期过滤由宿主执行，不能依赖 Agent 再次上线。

## 不确定的行为只形成候选

一次选择“正式一点”不证明用户永远喜欢正式表达。如果多次独立任务出现一致信号，
保留限定范围并通过 `propose` 提交候选：

```json
{"operation":"propose","parameters":{"content":"在面向客户的周报中可能偏好正式表达。","signal":"preference"}}
```

引用资料中的“我喜欢”、自动化模板、助手自己的推断不算用户自述。
重复发送同一轮内容不算独立证据。宿主建议确认时，读取候选和当前 revision，
核对支持证据、反例及范围，再用 `candidate.resolve` 处理；普通维护不转交用户逐条确认。

## Agent 处理后台复核

先读取当前目录公布的 `iyw.memory.maintenance.read.v1` 和
`iyw.memory.review.resolve.v1` schema，然后只取少量待办：

```json
{"operation":"maintenance.read","parameters":{"limit":4}}
```

返回 `revision`、`pendingCount` 和 `reviews`。每条包括原文引用、原因和证据引用。
模型提出建议本身不证明它正确。核对当前指令和相关原始记忆：

- 有明确撤销、纠正或范围错误证据时应用停止使用。
- 建议仅基于年龄、低使用次数或无法证实的推测时驳回。
- 新旧规则其实分别属于不同场景时保留各自有效范围。
- 会影响当前任务但无法确定的冲突，再向用户作一次具体澄清。

```json
{
  "operation":"review.resolve",
  "parameters":{
    "id":"<当前 review.id>",
    "expectedRevision":"<本次 maintenance.read 返回的 revision>",
    "apply":true
  }
}
```

驳回使用同一操作并将 `apply` 设为 `false`。看到 `resolved=true` 才算成功。
每次处置后 revision 可能改变，继续处理下一条前读取最新状态；不要用旧 revision 批量重放。
不要每轮扫描全库或对全部候选调用模型；在与任务有关的维护时机处理一个小批次。

## 三类内容的分工

- 用户记忆：事实和有范围的偏好，按相关性使用。
- 用户画像：由当前有效来源支持的归纳；来源消失后不能继续当作有效身份事实。
- 协作方式：表达、交付和执行偏好；当前任务明确要求优先。

`documents.read` 的原文用于编辑，它同时返回 `inactiveEntryIds`。这些已停用条目不能参与决策。
自动视图不得静默覆盖用户手写内容，也不能将自己的摘要重新当成新的独立证据。

## 失败时怎样继续

| 返回情况 | 处理 |
| --- | --- |
| `matched` | 检查任务范围、当前版本和相关性后使用 |
| `no_evidence` | 本次未找到匹配，不能编造过去的用户要求 |
| `unavailable`、云端服务或索引恢复中 | 使用当前任务证据继续，宿主负责后台恢复；不要要求用户开关检索、选择模型或操作模型文件 |
| revision/etag 冲突 | 重读当前条目，按新证据重建修改；不重放旧参数 |
| 执行结果未知 | 先查当前状态；不要换工具重复写入 |
| 当前会话无写权限 | 保持只读，不通过文件系统或其他身份绕过 |

“继续”首先使用当前会话的未完成任务；不能把其他项目的旧任务补成当前目标。
召回、注入、实际采用与结果核验是不同证据。Agent 说“完成”或正常 `end_turn`
不能自动成为任务成功或已验证经验；只有具体结果和验证依据才可记录为可复用经验。
