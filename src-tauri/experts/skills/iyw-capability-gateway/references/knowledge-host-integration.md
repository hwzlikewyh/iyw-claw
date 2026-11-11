# 知识库宿主接口：浏览、对话选择、资料维护与上传

供原助理现有界面／受授权客户端集成。桌面用现有transport.invoke，服务器用POST /api/<命令名>和相同JSON；成功直接返回对象／数组／null，失败沿AppCommandError。不要将这些宿主接口拼到gateway.iyw.cn云API前缀下。

云token、STS由宿主账号和上传器管理；Agent使用search_iyw_knowledge问答，用fetch_iyw_url做已支持的业务JSON操作，不通过接口读取凭据。

## 对话选择接口

| 命令名 | 请求JSON | 返回 |
| --- | --- | --- |
| knowledge_folders | {category:0或1} | Folder[]：folderId、folderName、fileCount、children |
| knowledge_files | {input:{category,folderId,page}} | list、total；此路径每页100 |
| knowledge_selection_get | {conversationId?:整数} | {scope,revision,selected} |
| knowledge_selection_set | {input:{conversationId,scope,revision,selected}} | 更新后Selection |

selected最多32项，每项：

```json
{"category":0,"kind":"folder","id":"<实际folderId>","name":"产品资料","folderId":"<同一实际folderId>"}
```

kind=file时id为真实fileId，folderId为其所属目录。category为0或1，kind仅folder／file；业务ID为1..128位十进制字符串，conversationId是宿主整数ID。

scope、revision从selection_get取得；更新时原样使用，不能自己生成scope或把revision归零。冲突则重新读并合并用户真实选择。作用域含当前账号／空间；切换账号后不能复用旧选择。

检索由search_iyw_knowledge解析当前活动对话的选择，覆盖子目录、去除重复范围，并在结果返回前核对scope／revision未变。未选范围的请求保持原可选过滤；选择不可用报错，不能退回整库。

## 资料库管理接口

资料库页面使用另一组接口，带scope防止账号切换覆盖；和上面的简化选择器分页不同。

| 命令名 | 请求JSON | 返回／限制 |
| --- | --- | --- |
| knowledge_library_folders | {category} | {scope,folders}；先读它取得当前资料库scope |
| knowledge_library_files | {input:{scope,category,folderId,page,searchText?}} | {list,total}；每页50，page≥1，searchText≤200字符 |
| knowledge_library_refresh | {input:{scope,category,fileIds,forceRefresh?}} | {list}；每批1..10文件 |
| knowledge_library_file_update | {input:{scope,category,fileId,fileName?,folderId?}} | 成功null；至少一项属性 |
| knowledge_library_file_delete | {input:{scope,category,fileId}} | 成功null |
| knowledge_library_points | {input:{scope,category,fileId,offset?,limit?}} | 云端切片对象；limit默认100且1..100 |
| knowledge_library_point_create | {input:<PointMutation>} | 成功null |
| knowledge_library_point_update | {input:<PointMutation>} | 成功null |
| knowledge_library_point_delete | {input:<PointMutation>} | 成功null |

PointMutation={scope,category,fileId,pointId?,chunkType?,chunkTitle?,content?,question?}。create用chunkType=text且content／question至少一个；update／delete必须有真实pointId。fileName非空≤180字符且无控制字符；目录ID必须为当前库真实存在的目录。

list条目可含fileId、docId、fileName、folderId、fileSize、fileExt、uploadEmployName、updatedAt、docProcessStatus／Message、fileUrl、tosUrl、syncStatus／Message、pointNum。fileId与docId用途不同，不能混填。

## 上传：5个专用命令

所有请求均用{input:{...}}包装，不是multipart：

| 命令名 | input | 结果 |
| --- | --- | --- |
| knowledge_upload_begin | {uploadId,scope,category,folderId,fileName,mimeType,expectedBytes} | UploadStatus；创建暂存上传 |
| knowledge_upload_append | {uploadId,scope,offset,dataBase64} | UploadStatus；按准确offset追加一块字节 |
| knowledge_upload_start | {uploadId,scope} | UploadStatus；开始宿主直传／注册 |
| knowledge_upload_status | {uploadId,scope} | 当前UploadStatus |
| knowledge_upload_abort | {uploadId,scope} | 成功null；不能据此声明已注册云文件已删 |

uploadId由客户端生成合法UUID；不是云端文件ID。scope取knowledge_library_folders返回值。expectedBytes为原文件实际总字节，mimeType使用真实类型。文件名≤180字符，不能含路径分隔符、控制字符。

前端现有流程的结构示例：

```typescript
const uploadId = crypto.randomUUID()
await invoke("knowledge_upload_begin", {
  input: {
    uploadId, scope, category, folderId,
    fileName: file.name,
    mimeType: file.type || "application/octet-stream",
    expectedBytes: file.size,
  },
})
```

接着按512KiB原始字节一块读取，转换dataBase64，offset取已确认的receivedBytes。最后一块可更小；Base64编码长度不能用作字节偏移。每块等待确认后再追加下一块；不要把整个大文件放入一次调用。

全部字节接收完成后start；未到expectedBytes不能启动。状态响应：

```json
{"uploadId":"<UUID>","phase":"receiving","receivedBytes":0,"sentBytes":0,"totalBytes":12345,"error":null}
```

phase=receiving／uploading／registering／complete／failed／cancelled。complete表示对象已上传且知识记录注册流程成功，不表示云端解析、同步或问答验证完成。继续用资料库列表和状态刷新确认。

同一宿主最多10个暂存上传，暂存字节总量≤2GiB，单文件上限按格式。收到offset或状态冲突先status核对，不能盲目重发append。注册失败且请求结果未知时复用宿主核对流程，不能另发files/create制造副本。

### 格式上限

| 类型 | 宿主限制 |
| --- | --- |
| PDF | 1GiB |
| doc／docx／epub／html／htm | 350MiB |
| ppt／pptx | 200MiB |
| md／markdown／txt／vtt | 10MiB |
| jpg／jpeg／png／webp／bmp／tiff／ico／dib／icns／sgi／jp2 | 8MiB，宽高10..6000、比例0.01..100 |
| mp4／mkv／avi／mov／wmv | 512MiB |
| csv／xlsx／jsonl／bitable | 1GiB客户端传输保护；云解析支持和上限仍待服务确认 |
| 音频及其余未声明格式 | 当前宿主拒绝 |

原云页面历史资料写MB／GB；本表为当前宿主1024进制常量，不能把两者不加区分当成供应商保证。

## 状态失效与故障处理

- scope不匹配：停止旧页操作，重新取得当前scope和列表；不把旧结果写进新账号页面。
- revision冲突：重读选择并合并，不覆盖用户的新选择。
- 上传失败：显示error，保留正确的uploadId和偏移证据；遵循当前上传器重试状态，不能假设跨进程／重启可恢复。
- registering超时：先核对目标列表与原对象；注册成功但响应丢失不可重复create。
- complete后仍无检索结果：检查docProcessStatus、syncStatus、pointNum和切片，不重复上传。

来源：web/router.rs、web/handlers/{knowledge,knowledge_library}.rs；commands/knowledge/{models,api,library,selection,search,results,upload,upload_models,upload_worker}.rs。
