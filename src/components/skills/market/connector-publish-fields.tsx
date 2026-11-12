import { useState } from "react"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { Button } from "@/components/ui/button"
import {
  CONNECTOR_PLATFORMS,
  type ConnectorOption,
} from "@/lib/connector-publishing"

export function ConnectorJSONField({
  label,
  value,
  onChange,
  onValidity,
}: {
  label: string
  value: unknown
  onChange: (value: unknown) => void
  onValidity: (valid: boolean) => void
}) {
  const [text, setText] = useState(() => JSON.stringify(value, null, 2))
  const [error, setError] = useState("")
  return (
    <label className="grid gap-1 text-sm">
      {label}
      <Textarea
        rows={5}
        value={text}
        className="font-mono text-xs"
        aria-invalid={Boolean(error)}
        onChange={(event) => {
          setText(event.target.value)
          try {
            const parsed = JSON.parse(event.target.value)
            onValidity(true)
            onChange(parsed)
            setError("")
          } catch {
            onValidity(false)
            setError("JSON格式无效，修正后才能保存")
          }
        }}
      />
      {error && (
        <span role="alert" className="text-xs text-destructive">
          {error}
        </span>
      )}
    </label>
  )
}

export function ConnectorOptionFields({
  option,
  index,
  change,
  remove,
  upload,
  validity,
}: {
  option: ConnectorOption
  index: number
  change: (option: ConnectorOption) => void
  remove: () => void
  upload: (file: File) => void
  validity: (key: string, valid: boolean) => void
}) {
  const [file, setFile] = useState<File | null>(null)
  const patch = (value: Partial<ConnectorOption>) =>
    change({ ...option, ...value })
  const protocol = (value: ConnectorOption["protocol"]) => {
    if (!window.confirm("切换协议会重置连接配置与认证方式，继续？")) return
    patch({
      protocol: value,
      spec:
        value === "stdio"
          ? {
              type: value,
              command: "node",
              args: ["${ARTIFACT_DIR}/server.js"],
            }
          : { type: value, url: "" },
      auth_mode: "none",
      cli_auth: undefined,
      artifact_id: "",
    })
  }
  const auth =
    option.protocol === "stdio"
      ? [
          ["none", "无需账号"],
          ["env", "环境参数"],
          ["cli", "CLI登录"],
        ]
      : [
          ["none", "无需账号"],
          ["header", "请求头凭据"],
          ["oauth", "浏览器OAuth"],
        ]
  const checkJSON = (name: string, value: unknown) => {
    const valid =
      name === "parameters"
        ? Array.isArray(value)
        : value != null && typeof value === "object" && !Array.isArray(value)
    validity(name, valid)
    if (valid) patch({ [name]: value })
  }
  return (
    <section className="space-y-3 rounded-lg border p-4">
      <div className="flex items-center gap-3">
        <h3 className="mr-auto text-sm font-semibold">接入方式 {index + 1}</h3>
        <Button variant="outline" type="button" size="sm" onClick={remove}>
          移除此方式
        </Button>
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        <label className="grid gap-1 text-sm">
          方式标识
          <Input
            required
            value={option.id}
            onChange={(event) => patch({ id: event.target.value })}
          />
        </label>
        <label className="grid gap-1 text-sm">
          显示名称
          <Input
            required
            value={option.label}
            onChange={(event) => patch({ label: event.target.value })}
          />
        </label>
        <label className="grid gap-1 text-sm">
          协议
          <select
            className="h-10 rounded-md border bg-background px-2"
            value={option.protocol}
            onChange={(event) =>
              protocol(event.target.value as ConnectorOption["protocol"])
            }
          >
            <option value="http">Streamable HTTP</option>
            <option value="sse">HTTP + SSE</option>
            <option value="stdio">STDIO 本地组件</option>
          </select>
        </label>
        <label className="grid gap-1 text-sm">
          认证方式
          <select
            className="h-10 rounded-md border bg-background px-2"
            value={option.auth_mode}
            onChange={(event) =>
              patch({ auth_mode: event.target.value, cli_auth: undefined })
            }
          >
            {auth.map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
        </label>
        <label className="grid gap-1 text-sm">
          所需运行时
          <Input
            value={option.runtime}
            onChange={(event) => patch({ runtime: event.target.value })}
            placeholder="如 Node.js >= 22"
          />
        </label>
        <label className="grid gap-1 text-sm">
          适用智能体（逗号分隔，可多选）
          <Input
            value={option.agents.join(", ")}
            onChange={(event) =>
              patch({
                agents: event.target.value
                  .split(",")
                  .map((value) => value.trim())
                  .filter(Boolean),
              })
            }
            placeholder="codex, claude_code"
          />
        </label>
      </div>
      <fieldset>
        <legend className="mb-2 text-sm">
          适用系统与架构（可多选；未选表示不限）
        </legend>
        <div className="flex flex-wrap gap-3">
          {CONNECTOR_PLATFORMS.map(([value, label]) => (
            <label
              key={value}
              className="flex min-h-8 items-center gap-2 text-xs"
            >
              <input
                type="checkbox"
                checked={option.platforms.includes(value)}
                onChange={(event) =>
                  patch({
                    platforms: event.target.checked
                      ? [...option.platforms, value]
                      : option.platforms.filter(
                          (platform) => platform !== value
                        ),
                  })
                }
              />
              {label}
            </label>
          ))}
        </div>
      </fieldset>
      <ConnectorJSONField
        key={`spec:${option.protocol}`}
        label="启动 / 连接配置 JSON"
        value={option.spec}
        onChange={(value) => checkJSON("spec", value)}
        onValidity={(valid) => validity("spec", valid)}
      />
      <ConnectorJSONField
        label="用户参数声明 JSON（只声明字段，不填写实际密钥）"
        value={option.parameters}
        onChange={(value) => checkJSON("parameters", value)}
        onValidity={(valid) => validity("parameters", valid)}
      />
      {option.auth_mode === "cli" && (
        <ConnectorJSONField
          label="CLI登录声明（login、status及可选logout）"
          value={
            option.cli_auth || {
              login: { command: "", args: [] },
              status: { command: "", args: [] },
            }
          }
          onChange={(value) => checkJSON("cli_auth", value)}
          onValidity={(valid) => validity("cli_auth", valid)}
        />
      )}
      {option.protocol === "stdio" && (
        <div className="grid gap-2">
          <label className="grid gap-1 text-sm">
            组件文件或ZIP（无固定包大小上限）
            <Input
              type="file"
              onChange={(event) => setFile(event.target.files?.[0] ?? null)}
            />
          </label>
          <Button
            variant="outline"
            type="button"
            className="w-fit"
            disabled={!file}
            onClick={() => file && upload(file)}
          >
            上传 / 重试当前文件
          </Button>
          <p className="break-all text-xs text-muted-foreground">
            {option.artifact_id
              ? `已登记制品：${option.artifact_id}`
              : "上传文件不会执行代码；发布后其他用户自行选择下载。"}
          </p>
        </div>
      )}
    </section>
  )
}
