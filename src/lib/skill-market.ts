import { getTransport } from "@/lib/transport"
import type { AgentSkillItem, AgentType } from "@/lib/types"

export type SkillMarketVisibility = "public" | "private"
export type SkillMarketPublisher = "official" | "user"
export type SkillMarketView = "market" | "mine"

export interface SkillMarketCategory {
  key: string
  fallbackName: string
  sortOrder: number
}

export interface SkillMarketVersion {
  id: string
  version: string
  changelog: string | null
  status: "ready" | "failed"
  fileCount: number
  packageSize: number
  createdAt: string
}

export interface SkillMarketFile {
  path: string
  size: number
  sha256: string
  mimeType: string | null
}

export interface SkillMarketItem {
  id: string
  slug: string
  displayName: string
  summary: string
  category: string
  iconUrl: string | null
  tags: string[]
  visibility: SkillMarketVisibility
  publisherType: SkillMarketPublisher
  currentVersion: SkillMarketVersion
  ownedByMe: boolean
  canManage: boolean
  createdAt: string
  updatedAt: string
}

export interface SkillMarketDetail extends SkillMarketItem {
  files: SkillMarketFile[]
}

export interface SkillMarketListResult {
  items: SkillMarketItem[]
  total: number
  page: number
  pageSize: number
}

export interface SkillMarketListParams {
  view: SkillMarketView
  visibility?: SkillMarketVisibility | "all"
  publisherType?: SkillMarketPublisher | "all"
  category?: string | null
  q?: string
  page?: number
  pageSize?: number
}

export interface SkillMarketUploadFile {
  path: string
  contentBase64: string
  size: number
}

export interface SelectedSkillMarketFolder {
  name: string
  files: SkillMarketUploadFile[]
  totalBytes: number
}

export interface SkillMarketPublishRequest {
  slug: string
  displayName: string
  summary: string
  category: string
  iconUrl: string | null
  tags: string[]
  visibility: SkillMarketVisibility
  version: string
  changelog: string
  files: SkillMarketUploadFile[]
}

export interface SkillMarketMetadataRequest {
  id: string
  displayName: string
  summary: string
  category: string
  iconUrl: string | null
  tags: string[]
  visibility: SkillMarketVisibility
}

export interface SkillMarketAddVersionRequest {
  id: string
  version: string
  changelog: string
  files: SkillMarketUploadFile[]
}

export type MarketInstalledSkill = AgentSkillItem & {
  market_managed?: boolean
  market_skill_id?: string | null
  installed_version?: string | null
  market_visibility?: SkillMarketVisibility | null
  publisher_type?: SkillMarketPublisher | null
  marketManaged?: boolean
  marketSkillId?: string | null
  installedVersion?: string | null
}

export function getInstalledMarketInfo(skill: AgentSkillItem): {
  managed: boolean
  marketId: string | null
  version: string | null
} {
  const item = skill as MarketInstalledSkill
  const managed = Boolean(
    item.market_managed ?? item.marketManaged ?? item.official
  )
  return {
    managed,
    marketId:
      item.market_skill_id ??
      item.marketSkillId ??
      (item.official ? item.id : null),
    version: item.installed_version ?? item.installedVersion ?? null,
  }
}

const MAX_FILES = 512
const MAX_BYTES = 25 * 1024 * 1024
const MAX_SKILL_MD_BYTES = 1024 * 1024
const INTERNAL_MARKERS = new Set([
  ".iyw-claw-market-skill.json",
  ".iyw-claw-official-skill.json",
])

function bytesToBase64(bytes: Uint8Array): string {
  let binary = ""
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000))
  }
  return btoa(binary)
}

function validatePath(path: string): void {
  const parts = path.split("/")
  if (
    !path ||
    path.includes("\\") ||
    path.includes("\0") ||
    path.startsWith("/") ||
    /^[a-zA-Z]:/.test(path) ||
    parts.some((part) => !part || part === "." || part === "..") ||
    new TextEncoder().encode(path).length > 512 ||
    INTERNAL_MARKERS.has(path)
  ) {
    throw new Error("invalidPath")
  }
}

function validatePathConflicts(paths: string[]): void {
  const files = new Set<string>()
  const directories = new Set<string>()
  for (const path of paths) {
    const folded = path.toLocaleLowerCase("en-US")
    if (files.has(folded)) throw new Error("duplicatePath")
    const parts = folded.split("/")
    for (let index = 1; index < parts.length; index += 1) {
      const directory = parts.slice(0, index).join("/")
      if (files.has(directory)) throw new Error("pathConflict")
      directories.add(directory)
    }
    if (directories.has(folded)) throw new Error("pathConflict")
    files.add(folded)
  }
}

export async function readSkillMarketFolder(
  selectedFiles: File[]
): Promise<SelectedSkillMarketFolder> {
  if (!selectedFiles.length) throw new Error("emptyFolder")
  if (selectedFiles.length > MAX_FILES) throw new Error("tooManyFiles")
  const entries = selectedFiles.map((file) => ({
    file,
    parts: file.webkitRelativePath.replace(/\\/g, "/").split("/"),
  }))
  const name = entries[0]?.parts[0] ?? ""
  if (
    !name ||
    entries.some(({ parts }) => parts[0] !== name || parts.length < 2)
  ) {
    throw new Error("invalidFolder")
  }
  const paths = entries.map(({ parts }) => parts.slice(1).join("/"))
  paths.forEach(validatePath)
  validatePathConflicts(paths)
  const totalBytes = entries.reduce((total, { file }) => total + file.size, 0)
  if (totalBytes > MAX_BYTES) throw new Error("folderTooLarge")
  const entryIndex = paths.indexOf("SKILL.md")
  if (entryIndex < 0) throw new Error("missingSkillFile")
  const skillFile = entries[entryIndex].file
  if (skillFile.size > MAX_SKILL_MD_BYTES) throw new Error("skillFileTooLarge")
  const skillBytes = new Uint8Array(await skillFile.arrayBuffer())
  try {
    if (!new TextDecoder("utf-8", { fatal: true }).decode(skillBytes).trim()) {
      throw new Error("emptySkillFile")
    }
  } catch (error) {
    if (error instanceof Error && error.message === "emptySkillFile")
      throw error
    throw new Error("invalidSkillFile")
  }
  const files = await Promise.all(
    entries.map(async ({ file }, index) => ({
      path: paths[index],
      contentBase64: bytesToBase64(
        index === entryIndex
          ? skillBytes
          : new Uint8Array(await file.arrayBuffer())
      ),
      size: file.size,
    }))
  )
  return { name, files, totalBytes }
}

export const skillMarketList = (params: SkillMarketListParams) =>
  getTransport().call<SkillMarketListResult>("skill_market_list", { params })

export const skillMarketCategories = () =>
  getTransport().call<SkillMarketCategory[]>("skill_market_categories")

export const skillMarketDetail = (id: string, version?: string | null) =>
  getTransport().call<SkillMarketDetail>("skill_market_detail", {
    id,
    version: version ?? null,
  })

export const skillMarketListVersions = (id: string) =>
  getTransport().call<SkillMarketVersion[]>("skill_market_list_versions", {
    id,
  })

export const skillMarketPublish = (request: SkillMarketPublishRequest) =>
  getTransport().call<SkillMarketDetail>("skill_market_publish", { request })

export const skillMarketAddVersion = (request: SkillMarketAddVersionRequest) =>
  getTransport().call<SkillMarketDetail>("skill_market_add_version", {
    request,
  })

export const skillMarketUpdateMetadata = (
  request: SkillMarketMetadataRequest
) =>
  getTransport().call<SkillMarketDetail>("skill_market_update_metadata", {
    request,
  })

export const skillMarketDelete = (id: string) =>
  getTransport().call<void>("skill_market_delete", { id })

export const skillMarketInstall = (
  id: string,
  version: string,
  agentType: AgentType
) =>
  getTransport().call<void>("skill_market_install", { id, version, agentType })
