import { getTransport } from "@/lib/transport"

export interface ConnectorOption {
  id: string
  label: string
  protocol: "http" | "sse" | "stdio"
  spec: Record<string, unknown>
  parameters: unknown[]
  platforms: string[]
  agents: string[]
  protocol_versions: string[]
  auth_mode: string
  runtime: string
  artifact_id: string
  cli_auth?: unknown
}
export interface OwnedConnector {
  server_id: string
  name: string
  description: string
  owner: string
  category: string
  latest_version: string
  instructions: string
  homepage?: string
  icon_url?: string
  examples: string[]
  install_options: ConnectorOption[]
  revision: number
  listed: boolean
  published_version: string
}
export interface OwnedConnectorPage {
  items: OwnedConnector[]
  total: number
  page: number
  pageSize: number
}
export interface ConnectorUploadTask {
  uploadToken: string
  url: string
  method: string
  headers?: Record<string, string>
  expiresAt: string
}
export interface ConnectorArtifact {
  id: string
  sha256: string
  size: number
  filename: string
}
export type ConnectorUserAction =
  | "mine"
  | "detail-owned"
  | "save"
  | "publish"
  | "unpublish"
  | "uploads/init"
  | "uploads/complete"
  | "uploads/abort"
export function connectorUserRequest<T>(
  action: ConnectorUserAction,
  body: unknown
): Promise<T> {
  return getTransport().call(
    "connector_market_user_request",
    { request: { action, body } },
    { timeoutMs: 10 * 60_000 }
  )
}
export function newConnectorOption(index = 1): ConnectorOption {
  return {
    id: `connection-${index}`,
    label: `接入方式 ${index}`,
    protocol: "http",
    spec: { type: "http", url: "" },
    parameters: [],
    platforms: [],
    agents: [],
    protocol_versions: [],
    auth_mode: "none",
    runtime: "",
    artifact_id: "",
  }
}
export function newOwnedConnector(): OwnedConnector {
  return {
    server_id: "",
    name: "",
    description: "",
    owner: "",
    category: "",
    latest_version: "1.0.0",
    instructions: "",
    examples: [],
    install_options: [newConnectorOption()],
    revision: 0,
    listed: false,
    published_version: "",
  }
}
export function importConnector(text: string): OwnedConnector {
  const value = JSON.parse(text)
  if (
    !isRecord(value) ||
    !Array.isArray(value.install_options) ||
    value.install_options.length < 1 ||
    value.install_options.length > 16
  )
    throw Error("连接器声明需包含 1–16 项 install_options")
  validateImportedFields(
    value,
    [
      "server_id",
      "name",
      "description",
      "owner",
      "category",
      "latest_version",
      "instructions",
      "homepage",
      "icon_url",
    ],
    ["examples"]
  )
  const result = {
    ...newOwnedConnector(),
    ...value,
    revision: 0,
    listed: false,
    published_version: "",
  }
  // 导入外部声明创建自己的新条目，原作者命名空间不能被认领。
  result.server_id = String(value.server_id || "").replace(
    /^user\.[a-f0-9]{16}\./,
    ""
  )
  result.install_options = value.install_options.map(importOption)
  return result
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value != null && typeof value === "object" && !Array.isArray(value)
}

function validateImportedFields(
  value: Record<string, unknown>,
  strings: string[],
  lists: string[]
) {
  for (const key of strings)
    if (key in value && typeof value[key] !== "string")
      throw Error(`连接器声明的 ${key} 必须为文本`)
  for (const key of lists) {
    if (!(key in value)) continue
    const items = value[key]
    if (!Array.isArray(items) || items.some((item) => typeof item !== "string"))
      throw Error(`连接器声明的 ${key} 必须为文本数组`)
  }
}

function importOption(value: unknown, index: number): ConnectorOption {
  if (!isRecord(value)) throw Error("接入方式必须为 JSON 对象")
  validateImportedFields(
    value,
    ["id", "label", "protocol", "auth_mode", "runtime", "artifact_id"],
    ["platforms", "agents", "protocol_versions"]
  )
  const option = { ...newConnectorOption(index + 1), ...value }
  if (!(["http", "sse", "stdio"] as string[]).includes(option.protocol))
    throw Error("接入协议必须为 http、sse 或 stdio")
  if (!isRecord(option.spec) || !Array.isArray(option.parameters))
    throw Error("连接配置必须为 JSON 对象，参数声明必须为数组")
  if (option.cli_auth != null && !isRecord(option.cli_auth))
    throw Error("CLI 登录声明必须为 JSON 对象")
  return option
}
export const CONNECTOR_PLATFORMS = [
  ["windows-x86_64", "Windows x64"],
  ["windows-aarch64", "Windows ARM64"],
  ["macos-x86_64", "macOS Intel"],
  ["macos-aarch64", "macOS Apple Silicon"],
  ["linux-x86_64", "Linux x64"],
  ["linux-aarch64", "Linux ARM64"],
] as const
