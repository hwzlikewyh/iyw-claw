# 资产库、作品证书与存储回执

来源：2026-09-10《爱原物接口文档·分层详版（补充篇）》；前端反解和原采集者实测标记，不表示本次已调用。只读取当前任务相关小节；其他领域见 [接口索引](iyw-api-index.md)，冲突和认证/签名边界见 [调用约定补充](iyw-api-access-contracts.md)。

本域业务统一 fetch_iyw_url。saas-assets 的完整地址必须是 https://www.iyw.cn/api/saas-assets/...；/oss/policy 是 https://www.iyw.cn/api/oss/policy。主机为 www.iyw.cn/api/ 注入当前登录 Cookie；不会将此 Cookie 发往其他域名。

文件上传用 upload_iyw_file，取得 URL 不等于创建资产或知识库记录。batchUpload/completeUpload/recordCompleted 需要真实业务 fileId/objectKey，不能从公开 URL 猜对象 ID。上传凭证接口仅作内部流程索引，不返回 STS 给代理。clearFileResource 是清空整个资源集，普通上传/查询任务不得附带调用。写成功后按来源刷新容量，而不是把上传成功当余额/配额已更新。

## 来源详细资料

### 9.6 云存储 / 上传凭证（4 个）

| 请求类型 | 接口 | 功能 | 关键入参 |
| --- | --- | --- | --- |
| POST | `/platform/oss/securityToken` | 阿里云 OSS STS（带 `sign:true`、`saasToken:true`） | 业务参数 |
| POST | `/platform/oss/securityNewToken` | OSS STS（新版） | 业务参数 |
| GET | `/ai-application/api/microModel/stsToken` | 图片存储 STS（带签名） | — |
| POST | `/ai-application/api/microModel/PreSignedUrl` | 上传预签名 URL | `fileName`、`fileType` |
| GET | `/user-service/aliyun/oss/policy` | 用户侧 OSS 上传策略 | — |
| POST | `/saas-assets/aliyun/oss/policy` | 资产库 OSS 上传策略 | `dir` 等 |
| POST | `/oss/policy` | 门户 OSS 上传策略（走 `/api` 代理） | `dir` |
| POST | `/saas-assets/aliyun/oss/recordCompleted` | 上传完成回执 | `objectKey`、`fileId` |

### 9.7 资产库（`/saas-assets/*`，挂在 `/api` 前缀下）

`i_app` 里前缀常量 `i = "/saas-assets"`，实际请求走 axios 实例 `baseURL:"/api"`，即最终地址是 `https://www.iyw.cn/api/saas-assets/...`；其中 8 个写操作在成功后会自动刷新 `user/getStorage`：`capacity/clearFileResource`、`art/batchUpload`、`art/completeUpload`、`art/del`、`certificate/del`、`certificate/modifyUploadedCertificate`、`art/copyFile`。

| 接口 | 功能 | 关键入参 |
| --- | --- | --- |
| `/saas-assets/capacity/queryOrgCapacityInfo` | 机构容量 | — |
| `/saas-assets/capacity/queryMemberCapacityUsage` | 成员容量使用 | `memberId` |
| `/saas-assets/capacity/queryOrgArtTotalNumAndCertTotalNum` | 作品/证书总数 | — |
| `/saas-assets/capacity/clearFileResource` | **清空文件资源**（写） | — |
| `/saas-assets/art/queryFileFormat` | 支持的格式 | — |
| `/saas-assets/art/queryArtList` | 作品列表 | 分页 |
| `/saas-assets/art/queryFolderPath` | 文件夹路径 | `folderId` |
| `/saas-assets/art/createFolder` | 新建文件夹 | `name`、`parentId` |
| `/saas-assets/art/batchUpload` | 批量上传 | 文件信息（写） |
| `/saas-assets/art/completeUpload` | 上传完成 | `fileId`（写） |
| `/saas-assets/art/queryArtInfo` | 作品详情 | `artId` |
| `/saas-assets/art/queryExpireDetail` | 到期详情 | `artId` |
| `/saas-assets/art/queryFolderList` | 文件夹列表 | 分页 |
| `/saas-assets/art/rename` | 重命名 | `artId`、`name` |
| `/saas-assets/art/copyFile` | 复制文件 | `artId`（写） |
| `/saas-assets/art/moveFile` | 移动文件 | `artId`、`folderId` |
| `/saas-assets/art/del` | **删除作品** | `artId`（写，未执行） |
| `/saas-assets/art/downloadFile` | 下载 | `artId` |
| `/saas-assets/art/getArtInfoList` | 批量取作品信息 | `artIds[]` |
| `/saas-assets/art/artCertificateRelation` | 作品-证书关联 | `artId` |
| `/saas-assets/art/checkArtName` | 校验作品名 | `name` |
| `/saas-assets/art/queryArtCertificate` | 作品证书 | `artId` |
| `/saas-assets/art/queryArtCheck` | 作品审核状态 | `artId` |
| `/saas-assets/art/queryRetrain` | 取训练状态 | `artId` |
| `/saas-assets/art/saveRetrain` | 保存训练 | `artId` |
| `/saas-assets/art/queryTagList` | 标签列表 | `artId` |
| `/saas-assets/art/updateTagStatus` | 更新标签状态 | `tagId` |
| `/saas-assets/certificate/certificateList` | 证书列表 | 分页 |
| `/saas-assets/certificate/queryCertificateDetails` | 证书详情 | `certId` |
| `/saas-assets/certificate/uploadCertificate` | 上传证书 | 文件信息 |
| `/saas-assets/certificate/uploadCertificateList` | 批量上传证书 | 文件列表 |
| `/saas-assets/certificate/modifyUploadedCertificate` | 修改已上传证书 | `certId`（写） |
| `/saas-assets/certificate/del` | **删除证书** | `certId`（写，未执行） |
| `/saas-assets/certificate/downloadCertificate` | 下载证书 | `certId` |
| `/saas-assets/certificate/associateArt` | 关联作品 | `certId`、`artIds[]` |
| `/saas-assets/certificate/getAssociateArtListByCertId` | 关联作品列表 | `certId` |
| `/saas-assets/certificate/getBrandCertInfoByOCR` | OCR 识别品牌证书 | `imageUrl` |
| `/saas-assets/certificate/getCustomTypeNameByCertIds` | 证书自定义类型名 | `certIds[]` |
| `/saas-assets/certificate/queryExpireCertificate` | 到期证书列表 | 分页 |
| `/saas-assets/certificate/queryExpireCertificateTotal` | 到期证书数量 | — |
| `/saas-assets/certificate/renameCertificate` | 重命名证书 | `certId`、`name` |
| `/saas-assets/customType/addOrUpdate` / `edit` / `del` / `queryTypeList` / `updateCustomTypeSequence` | 自定义分类增删改查 + 排序 | `typeId` / `name` / `sequence` |
| `/saas-assets/member/addOrUpdate` / `del` / `queryMemberList` / `queryAllMemberList` | 成员增删改查 | `memberId` / 分页 |
