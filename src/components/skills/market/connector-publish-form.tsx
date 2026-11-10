import { useState } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import {
  newConnectorOption,
  type OwnedConnector,
} from "@/lib/connector-publishing"
import type { useConnectorPublishing } from "./use-connector-publishing"
import { ConnectorOptionFields } from "./connector-publish-fields"

export function ConnectorPublishForm({
  editor,
}: {
  editor: ReturnType<typeof useConnectorPublishing>
}) {
  const draft = editor.draft!
  const [invalid, setInvalid] = useState<Record<string, boolean>>({})
  const patch = (value: Partial<OwnedConnector>) =>
    editor.change({ ...draft, ...value })
  const field = (
    key: "server_id" | "name" | "owner" | "latest_version" | "category",
    label: string
  ) => (
    <label key={key} className="grid gap-1 text-sm">
      {label}
      <Input
        required={["name", "owner", "latest_version"].includes(key)}
        disabled={key === "server_id" && draft.revision > 0}
        value={draft[key]}
        onChange={(event) => patch({ [key]: event.target.value })}
      />
    </label>
  )
  return (
    <form
      onSubmit={(event) => {
        event.preventDefault()
        if (!Object.values(invalid).some(Boolean)) editor.save()
      }}
      className="space-y-4"
    >
      <p className="text-xs text-muted-foreground">
        只管理当前账号的连接器。保存草稿与公开发布分开；上传不等于安装、登录或连接成功。
      </p>
      <fieldset disabled={editor.busy} className="space-y-4">
        <div className="grid gap-3 sm:grid-cols-2">
          {field("name", "名称")}
          {field("server_id", "标识后缀（服务端添加自己的命名空间）")}
          {field("latest_version", "版本，如1.0.0")}
          {field("owner", "发布者显示名称")}
          {field("category", "分类")}
          <label className="grid gap-1 text-sm">
            帮助地址（可选）
            <Input
              type="url"
              value={draft.homepage || ""}
              onChange={(event) => patch({ homepage: event.target.value })}
            />
          </label>
          <label className="grid gap-1 text-sm">
            图标地址（可选）
            <Input
              type="url"
              value={draft.icon_url || ""}
              onChange={(event) => patch({ icon_url: event.target.value })}
            />
          </label>
        </div>
        <label className="grid gap-1 text-sm">
          用途说明
          <Textarea
            required
            value={draft.description}
            onChange={(event) => patch({ description: event.target.value })}
          />
        </label>
        <label className="grid gap-1 text-sm">
          使用示例（每行一项）
          <Textarea
            value={draft.examples.join("\n")}
            onChange={(event) =>
              patch({ examples: event.target.value.split("\n") })
            }
          />
        </label>
        <label className="grid gap-1 text-sm">
          安装 / 认证说明
          <Textarea
            value={draft.instructions}
            onChange={(event) => patch({ instructions: event.target.value })}
          />
        </label>
        {draft.install_options.map((option, index) => (
          <ConnectorOptionFields
            key={index}
            option={option}
            index={index}
            change={(value) =>
              patch({
                install_options: draft.install_options.map(
                  (current, position) => (position === index ? value : current)
                ),
              })
            }
            remove={() => {
              if (draft.install_options.length > 1) {
                patch({
                  install_options: draft.install_options.filter(
                    (_, position) => position !== index
                  ),
                })
                setInvalid({})
              }
            }}
            upload={(file) => editor.upload(file, index)}
            validity={(name, valid) =>
              setInvalid((previous) => ({
                ...previous,
                [`${index}:${name}`]: !valid,
              }))
            }
          />
        ))}
        <Button
          variant="outline"
          type="button"
          disabled={draft.install_options.length >= 16}
          onClick={() =>
            patch({
              install_options: [
                ...draft.install_options,
                newConnectorOption(draft.install_options.length + 1),
              ],
            })
          }
        >
          添加接入方式 / 其他平台包
        </Button>
        <p className="text-xs text-muted-foreground">
          状态：
          {draft.listed
            ? "已发布"
            : draft.published_version
              ? "已下架 / 有草稿"
              : "草稿"}{" "}
          · 修订 {draft.revision}
        </p>
        <div className="flex flex-wrap gap-2">
          <Button type="submit" disabled={Object.values(invalid).some(Boolean)}>
            保存草稿
          </Button>
          <Button
            variant="outline"
            type="button"
            disabled={editor.dirty || !draft.revision}
            onClick={() => editor.publish(true)}
          >
            发布已保存版本
          </Button>
          {draft.listed && (
            <Button
              variant="outline"
              type="button"
              disabled={editor.dirty}
              onClick={() => editor.publish(false)}
            >
              下架
            </Button>
          )}
        </div>
      </fieldset>
      {editor.uploading != null && (
        <Button
          variant="outline"
          type="button"
          disabled={editor.uploadPhase === "verify"}
          onClick={() => void editor.cancelUpload()}
        >
          {editor.uploadPhase === "verify"
            ? "正在完成文件校验…"
            : "取消本次上传"}
        </Button>
      )}
    </form>
  )
}
