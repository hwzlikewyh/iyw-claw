import { getTransport } from "@/lib/transport"
import type { AgentSkillItem, AgentType } from "@/lib/types"

export type SkillMarketVisibility = "public" | "private"
export type SkillMarketPublisher = "official" | "user"
export type SkillMarketView = "market" | "mine"
export type SkillPackageType = "skill" | "expert" | "plugin"

export interface SkillPluginManifest {
  schemaVersion: number
  name: string
  version: string
  targets: Array<"codex" | "claude_code" | "iyw-claw">
  components: SkillPluginComponent[]
  bindings: SkillPluginBinding[]
  permissions?: SkillPluginPermissions | null
  manifestDigest?: string | null
}

export interface SkillPluginComponent {
  type: "skill" | "connector" | "runtime" | "capability" | "app"
  key: string
  path?: string
  serverKey?: string
  config?: Record<string, unknown> | null
}

export interface SkillPluginBinding {
  skillKey: string
  connectorKey: string
}

export interface SkillPluginPermissions {
  workspace: { read: string[]; write: string[] }
  network: {
    connectDomains: string[]
    resourceDomains: string[]
    frameDomains: string[]
  }
  host: Array<"send-message" | "clipboard-write" | "open-link">
}

export interface SkillDependency {
  skillId: string
  slug: string
  version: string
}

export interface SkillDependencyInput {
  slug: string
  version: string
}

export interface SkillMarketCategory {
  key: string
  fallbackName: string
  sortOrder: number
}

export interface SkillMarketVersion {
  id: string
  version: string
  changelog: string | null
  status: "ready" | "artifact_pending" | "failed"
  fileCount: number
  packageSize: number
  artifactSize?: number
  artifactSha256?: string | null
  failureCode?: string | null
  activeArtifactId?: string | null
  artifact?: SkillMarketArtifact | null
  packageType: SkillPackageType
  dependencies: SkillDependency[]
  plugin?: SkillPluginManifest | null
  createdAt: string
}

export interface SkillMarketArtifact {
  id: string
  generation: number
  status: "pending" | "building" | "ready" | "failed"
  rawSize: number
  artifactSize: number
  artifactSha256: string
  fileName: string
  packageKind: string
  failureCode?: string | null
  verifiedAt?: string | null
  createdAt: string
  updatedAt: string
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
  installedVersion?: string | null
  createdAt: string
  updatedAt: string
}

export interface SkillMarketDetail extends SkillMarketItem {
  files: SkillMarketFile[]
  installTargets?: AgentType[]
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
  packageType: Extract<SkillPackageType, "skill" | "plugin">
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
  packageType: SkillPackageType
  dependencies: SkillDependencyInput[]
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
  packageType: SkillPackageType
  dependencies: SkillDependencyInput[]
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
  const pluginIndex = paths.indexOf(".iyw-plugin.json")
  const packageType = pluginIndex >= 0 ? "plugin" : "skill"
  const entryIndex =
    packageType === "plugin" ? pluginIndex : paths.indexOf("SKILL.md")
  if (entryIndex < 0) throw new Error("missingSkillFile")
  const entryFile = entries[entryIndex].file
  const entryBytes = new Uint8Array(await entryFile.arrayBuffer())
  if (packageType === "plugin") {
    await validatePluginFolderEntry(entryBytes, name, paths)
  } else {
    validateSkillFolderEntry(entryBytes)
  }
  const files = await Promise.all(
    entries.map(async ({ file }, index) => ({
      path: paths[index],
      contentBase64: bytesToBase64(
        index === entryIndex
          ? entryBytes
          : new Uint8Array(await file.arrayBuffer())
      ),
      size: file.size,
    }))
  )
  return { name, packageType, files, totalBytes }
}

function validateSkillFolderEntry(bytes: Uint8Array): void {
  if (bytes.byteLength > MAX_SKILL_MD_BYTES)
    throw new Error("skillFileTooLarge")
  try {
    if (!new TextDecoder("utf-8", { fatal: true }).decode(bytes).trim()) {
      throw new Error("emptySkillFile")
    }
  } catch (error) {
    if (error instanceof Error && error.message === "emptySkillFile")
      throw error
    throw new Error("invalidSkillFile")
  }
}

async function validatePluginFolderEntry(
  bytes: Uint8Array,
  folderName: string,
  paths: string[]
): Promise<void> {
  if (bytes.byteLength > MAX_SKILL_MD_BYTES)
    throw new Error("skillFileTooLarge")
  const hasPortableManifest = paths.includes(".iyw-plugin.json")
  const hasNativeManifest =
    paths.includes(".codex-plugin/plugin.json") &&
    paths.includes(".claude-plugin/plugin.json")
  const hasNativeArtifacts = paths.some(
    (path) =>
      path === ".codex-plugin/plugin.json" ||
      path === ".claude-plugin/plugin.json"
  )
  if (!hasPortableManifest && !hasNativeManifest) {
    throw new Error("missingPluginManifest")
  }
  try {
    const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes)
    const manifest = JSON.parse(text) as Record<string, unknown>
    if (
      manifest.name !== folderName ||
      typeof manifest.version !== "string" ||
      !manifest.version
    ) {
      throw new Error("invalidPluginManifest")
    }
    if (manifest.schemaVersion === 2) {
      const targets = manifest.targets
      if (
        !hasPortableManifest ||
        hasNativeArtifacts ||
        !Array.isArray(targets) ||
        targets.length !== 1 ||
        targets[0] !== "iyw-claw" ||
        paths.includes(".mcp.json")
      ) {
        throw new Error("invalidPluginManifest")
      }
      return
    }
    if (manifest.schemaVersion !== 1 || !hasNativeManifest) {
      throw new Error("invalidPluginManifest")
    }
  } catch (error) {
    if (error instanceof Error && error.message === "invalidPluginManifest") {
      throw error
    }
    throw new Error("invalidPluginManifest")
  }
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
  agentTypes: AgentType[]
) =>
  getTransport().call<void>("skill_market_install", { id, version, agentTypes })

// ---------------------------------------------------------------------------
// Skill Market v2 display model (Task 10)
//
// The backend contract v2 is not frozen yet (Task 01/03 blocked): audience,
// distribution policy, artifact status, compatibility and install plans are
// modelled here as typed display state. The UI is developed against
// `SkillMarketSource` (see src/lib/skill-market-source.ts) which currently
// serves typed fixtures; the transport-backed implementation lands together
// with the frozen contract. Do not change backend fields from the UI.
// ---------------------------------------------------------------------------

export type SkillMarketAudience =
  | "global_market"
  | "organization"
  | "owner_private"

export type SkillMarketDistributionPolicy = "mandatory" | "optional"

export type SkillMarketCompatibility = "compatible" | "incompatible" | "unknown"

export type SkillMarketArtifactStatus = "ready" | "artifact_pending" | "failed"

export type SkillMarketInstallState =
  | "not_installed"
  | "installed"
  | "update_available"
  | "blocked"
  | "preparing"

export type SkillMarketSort = "recommended" | "updated" | "name" | "installed"

export type SkillMarketViewV2 =
  | "market"
  | "organization"
  | "mine"
  | "installed"
  | "needs_update"

export type SkillMarketInstallErrorCode =
  | "artifact_not_ready"
  | "client_incompatible"
  | "audience_denied"
  | "dependency_unavailable"
  | "version_blocked"
  | "plan_expired"
  | "catalog_stale"
  | "disk_full"
  | "download_failed"
  | "checksum_mismatch"
  | "signature_invalid"
  | "canceled"

export interface SkillMarketV2Version {
  id: string
  version: string
  changelog: string | null
  status: SkillMarketArtifactStatus
  fileCount: number
  /** ZIP artifact bytes. Never use rawSize for integrity decisions. */
  artifactSize: number
  rawSize?: number
  artifactSha256: string | null
  artifact?: SkillMarketArtifact | null
  dependencies: SkillDependency[]
  packageType: SkillPackageType
  plugin?: SkillPluginManifest | null
  releasedAt: string
  failureCode?: string | null
}

export interface SkillMarketV2Item {
  id: string
  slug: string
  displayName: string
  summary: string
  category: string
  iconUrl: string | null
  tags: string[]
  audience: SkillMarketAudience
  distributionPolicy: SkillMarketDistributionPolicy
  publisher: SkillMarketPublisher
  packageType: SkillPackageType
  currentVersion: SkillMarketV2Version
  compatibility: SkillMarketCompatibility
  installState: SkillMarketInstallState
  installedVersion: string | null
  canManage: boolean
  organizationName: string | null
  updatedAt: string
}

export interface SkillMarketV2FileNode {
  path: string
  name: string
  size: number
  directory: boolean
  sha256?: string | null
  children?: SkillMarketV2FileNode[]
}

export interface SkillMarketV2CompatibilityDetail {
  minClientVersion: string | null
  osArch: string | null
  reason: string | null
  deadline: string | null
}

export type SkillMarketOwnershipSource = "system" | "market" | "user_dir"

export interface SkillMarketV2Detail extends SkillMarketV2Item {
  files: SkillMarketV2FileNode[]
  installTargets: AgentType[]
  ownership: {
    source: SkillMarketOwnershipSource
    managed: boolean
  }
  compatibilityDetail: SkillMarketV2CompatibilityDetail
}

export interface SkillMarketV2CatalogPage {
  items: SkillMarketV2Item[]
  nextCursor: string | null
  total: number
  catalogRevision: string
  offline: boolean
}

export interface SkillMarketListQueryV2 {
  view: SkillMarketViewV2
  publisher: SkillMarketPublisher | "all"
  distribution: SkillMarketDistributionPolicy | "all"
  compatibility: SkillMarketCompatibility | "all"
  category: string | null
  q: string
  sort: SkillMarketSort
  cursor: string | null
  limit: number
}

export interface SkillMarketInstallPlanItemV2 {
  skillId: string
  versionId: string
  artifactId: string
  slug: string
  displayName: string
  version: string
  audience: SkillMarketAudience
  distributionPolicy: SkillMarketDistributionPolicy
  artifactSize: number
  artifactSha256: string
  signature: string | null
  ticketEndpoint: string
  dependencies: SkillDependency[]
  packageType: SkillPackageType
  plugin?: SkillPluginManifest | null
}

export interface SkillMarketInstallPlanV2 {
  planId: string
  catalogRevision: string
  targetSkillId: string
  targetVersion: string
  items: SkillMarketInstallPlanItemV2[]
  totalBytes: number
  dependencyCount: number
  mandatory: boolean
}

export type SkillMarketInstallPhase =
  | "pending"
  | "downloading"
  | "verifying"
  | "extracting"
  | "activating"
  | "done"
  | "failed"
  | "canceled"

export interface SkillMarketInstallArtifactProgress {
  artifactId: string
  displayName: string
  version: string
  phase: SkillMarketInstallPhase
  bytesReceived: number
  bytesTotal: number
  errorCode: SkillMarketInstallErrorCode | null
  message: string | null
}

export type SkillMarketInstallOverall =
  | "idle"
  | "resolving"
  | "confirming"
  | "running"
  | "activating"
  | "done"
  | "failed"
  | "canceled"

export interface SkillMarketInstallSession {
  status: SkillMarketInstallOverall
  plan: SkillMarketInstallPlanV2 | null
  items: SkillMarketInstallArtifactProgress[]
  overallBytes: number
  receivedBytes: number
  errorCode: SkillMarketInstallErrorCode | null
  errorMessage: string | null
  startedAt: number | null
  /** Set while an expired ticket is being refreshed in the background. */
  refreshingTicket: boolean
  ticketRefreshCount: number
}

export type MarketBadgeTone =
  | "default"
  | "primary"
  | "success"
  | "warning"
  | "danger"
  | "muted"

export type MarketBadgeIcon =
  | "globe"
  | "building"
  | "lock"
  | "shield"
  | "check"
  | "arrowUp"
  | "clock"
  | "ban"
  | "alert"
  | "package"
  | "wrench"

export interface MarketBadgeInfo {
  /** i18n key relative to the `SkillMarketV2` namespace. */
  key: string
  tone: MarketBadgeTone
  icon?: MarketBadgeIcon
}

export function audienceBadgeInfo(
  audience: SkillMarketAudience
): MarketBadgeInfo {
  switch (audience) {
    case "global_market":
      return { key: "audience.globalMarket", tone: "primary", icon: "globe" }
    case "organization":
      return { key: "audience.organization", tone: "default", icon: "building" }
    case "owner_private":
      return { key: "audience.ownerPrivate", tone: "muted", icon: "lock" }
  }
}

export function distributionBadgeInfo(
  policy: SkillMarketDistributionPolicy
): MarketBadgeInfo {
  return policy === "mandatory"
    ? { key: "distribution.mandatory", tone: "warning", icon: "shield" }
    : { key: "distribution.optional", tone: "muted" }
}

export function compatibilityBadgeInfo(
  compatibility: SkillMarketCompatibility
): MarketBadgeInfo {
  switch (compatibility) {
    case "compatible":
      return { key: "compatibility.compatible", tone: "success", icon: "check" }
    case "incompatible":
      return { key: "compatibility.incompatible", tone: "danger", icon: "ban" }
    case "unknown":
      return { key: "compatibility.unknown", tone: "muted" }
  }
}

export function installStateBadgeInfo(
  state: SkillMarketInstallState
): MarketBadgeInfo {
  switch (state) {
    case "installed":
      return { key: "list.installed", tone: "success", icon: "check" }
    case "update_available":
      return { key: "list.updateAvailable", tone: "warning", icon: "arrowUp" }
    case "blocked":
      return { key: "list.blocked", tone: "danger", icon: "ban" }
    case "preparing":
      return { key: "list.preparing", tone: "muted", icon: "clock" }
    case "not_installed":
      return { key: "list.notInstalled", tone: "muted" }
  }
}

export function artifactStatusBadgeInfo(
  status: SkillMarketArtifactStatus
): MarketBadgeInfo {
  switch (status) {
    case "ready":
      return { key: "artifact.ready", tone: "success", icon: "check" }
    case "artifact_pending":
      return { key: "artifact.artifactPending", tone: "warning", icon: "clock" }
    case "failed":
      return { key: "artifact.failed", tone: "danger", icon: "alert" }
  }
}

export type SkillMarketPrimaryAction =
  | "install"
  | "update"
  | "reinstall"
  | "none"

/**
 * Resolves the primary button action. `unknown` compatibility is released
 * optimistically: the backend has the final say and answers with a
 * `client_incompatible` install error, which `installErrorAction` maps to
 * `update_client`. Only a definite `incompatible` blocks the button up front.
 */
export function primaryInstallAction(
  state: SkillMarketInstallState,
  compatibility: SkillMarketCompatibility
): SkillMarketPrimaryAction {
  if (compatibility === "incompatible") return "none"
  switch (state) {
    case "not_installed":
    case "preparing":
      return "install"
    case "update_available":
      return "update"
    case "installed":
      return "reinstall"
    case "blocked":
      return "none"
  }
}

export type SkillMarketErrorAction =
  | "retry"
  | "free_space"
  | "update_client"
  | "contact_admin"
  | "diagnostics"

export function installErrorAction(
  code: SkillMarketInstallErrorCode
): SkillMarketErrorAction {
  switch (code) {
    case "disk_full":
      return "free_space"
    case "client_incompatible":
    case "version_blocked":
      return "update_client"
    case "artifact_not_ready":
    case "audience_denied":
    case "dependency_unavailable":
      return "contact_admin"
    case "download_failed":
    case "checksum_mismatch":
    case "signature_invalid":
    case "plan_expired":
    case "catalog_stale":
    case "canceled":
      return "retry"
  }
}

export function formatSkillBytes(bytes: number): string {
  const safe = Math.max(0, Number(bytes) || 0)
  if (safe < 1024) return `${safe} B`
  const units = ["KB", "MB", "GB"]
  let value = safe / 1024
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  return `${value >= 100 ? Math.round(value) : value.toFixed(1)} ${units[unit]}`
}

/** Dynamic-key translator for the `SkillMarketV2` namespace (badges/selects). */
export type SkillMarketTranslator = (
  key: string,
  values?: Record<string, string | number>
) => string
