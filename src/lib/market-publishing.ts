import {
  connectorUserRequest,
  type OwnedConnectorPage,
} from "./connector-publishing"
import { getTransport } from "./transport"
import { connectorFileDigest } from "./connector-upload-digest"
import { extractAppCommandError } from "./app-error"

const CHECK_PAGE_SIZE = 50
const normalizeName = (name: string) => name.trim().toLowerCase()

export function isCommittedMarketPublication(error: unknown): boolean {
  // Rust 仅在上传已提交、随后详情读取失败时添加此回执前缀。
  return (
    extractAppCommandError(error)?.detail?.startsWith("已提交；uploadId=") ??
    false
  )
}

export async function connectorPublishId(name: string): Promise<string> {
  const normalized = normalizeName(name)
  if (!normalized) throw Error("请输入连接器名称")
  const digest = await connectorFileDigest(new Blob([normalized]), {
    signal: new AbortController().signal,
    progress: () => {},
  })
  return `connector-${digest}`
}

export interface SkillPublicationInput {
  folderName: string
  displayName: string
  packageType: "skill" | "plugin"
}

export interface SkillPublicationCheck {
  slug: string
  nameAvailable: boolean
  slugAvailable: boolean
}

export async function prepareSkillPublication(request: SkillPublicationInput) {
  const result = await getTransport().call<SkillPublicationCheck>(
    "skill_market_check_publication",
    { request }
  )
  if (
    !/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(result.slug) ||
    result.slug.length > 128 ||
    typeof result.nameAvailable !== "boolean" ||
    typeof result.slugAvailable !== "boolean"
  ) {
    throw Error("Invalid publication check response")
  }
  return result
}

export async function checkConnectorPublishName(input: {
  name: string
  excludeId?: string
}): Promise<boolean> {
  const name = normalizeName(input.name)
  if (!name) return false
  let page = 1
  while (true) {
    const result = await connectorUserRequest<OwnedConnectorPage>("mine", {
      page,
      pageSize: CHECK_PAGE_SIZE,
      query: input.name.trim(),
      status: "all",
    })
    if (
      result.items.some(
        (item) =>
          item.server_id !== input.excludeId &&
          normalizeName(item.name) === name
      )
    )
      return true
    if (page * CHECK_PAGE_SIZE >= result.total) return false
    if (!result.items.length) throw Error("名称检查返回的列表不完整，请重试")
    page += 1
  }
}
