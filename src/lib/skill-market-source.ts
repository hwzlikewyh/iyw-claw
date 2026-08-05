import type {
  SkillMarketAddVersionRequest,
  SkillMarketCategory,
  SkillMarketInstallErrorCode,
  SkillMarketInstallPlanV2,
  SkillMarketInstallPlanItemV2,
  SkillMarketListQueryV2,
  SkillMarketPublishRequest,
  SkillMarketV2CatalogPage,
  SkillMarketV2Detail,
  SkillMarketV2FileNode,
  SkillMarketV2Item,
  SkillMarketV2Version,
  SkillMarketAudience,
  SkillMarketDistributionPolicy,
  SkillMarketItem,
  SkillMarketVersion,
  SkillMarketListParams,
} from "@/lib/skill-market"
import {
  skillMarketList,
  skillMarketCategories,
  skillMarketDetail,
  skillMarketListVersions,
  skillMarketPublish,
  skillMarketAddVersion,
  skillMarketUpdateMetadata,
  skillMarketDelete,
} from "@/lib/skill-market"
import { getTransport } from "@/lib/transport"
import { createFixtureSkillMarketSource } from "@/lib/skill-market-fixtures"
import { compareSemVer } from "@/components/skills/skill-market-semver"

// ---------------------------------------------------------------------------
// Skill Market data source seam (Task 10)
//
// The UI only talks to `SkillMarketSource`. The factory returns the
// transport-backed implementation, which maps the existing v1 `skill_market_*`
// commands onto the v2 display model. The typed fixture implementation stays
// available for list benchmarks via `perfCount` (e.g. ?perf=500). Contract v2
// fields the backend does not emit yet are derived conservatively (see
// mapAudience / mapVersionV1ToV2) until T01/T03 freeze the contract.
// ---------------------------------------------------------------------------

export interface SkillMarketPublishRequestV2 {
  slug: string
  displayName: string
  summary: string
  category: string
  iconUrl: string | null
  tags: string[]
  audience: SkillMarketAudience
  version: string
  changelog: string
  dependencies: SkillMarketPublishRequest["dependencies"]
  files: SkillMarketPublishRequest["files"]
}

export interface SkillMarketMetadataRequestV2 {
  id: string
  displayName: string
  summary: string
  category: string
  iconUrl: string | null
  tags: string[]
  audience: SkillMarketAudience
}

export interface SkillMarketAddVersionRequestV2 {
  id: string
  version: string
  changelog: string
  dependencies: SkillMarketAddVersionRequest["dependencies"]
  files: SkillMarketAddVersionRequest["files"]
}

export class SkillMarketSourceError extends Error {
  readonly code: SkillMarketInstallErrorCode

  constructor(code: SkillMarketInstallErrorCode, message?: string) {
    super(message ?? code)
    this.name = "SkillMarketSourceError"
    this.code = code
  }
}

export interface SkillMarketSource {
  list(query: SkillMarketListQueryV2): Promise<SkillMarketV2CatalogPage>
  categories(): Promise<SkillMarketCategory[]>
  detail(id: string, version?: string | null): Promise<SkillMarketV2Detail>
  versions(id: string): Promise<SkillMarketV2Version[]>
  files(id: string, version: string): Promise<SkillMarketV2FileNode[]>
  resolve(id: string, version: string): Promise<SkillMarketInstallPlanV2>
  publish(request: SkillMarketPublishRequestV2): Promise<SkillMarketV2Item>
  addVersion(
    request: SkillMarketAddVersionRequestV2
  ): Promise<SkillMarketV2Item>
  updateMetadata(
    request: SkillMarketMetadataRequestV2
  ): Promise<SkillMarketV2Item>
  delete(id: string): Promise<void>
  uninstall(id: string): Promise<void>
  rebuildArtifact(id: string, version: string): Promise<SkillMarketV2Version>
}

export interface SkillMarketSourceOptions {
  /** Number of synthetic catalog entries for list benchmarks (500/5000). */
  perfCount?: number
}

// ---------------------------------------------------------------------------
// v1 → v2 mapping helpers
// ---------------------------------------------------------------------------

function mapAudience(
  raw: unknown,
  visibility: string,
  publisherType: string
): SkillMarketAudience {
  if (
    raw === "global_market" ||
    raw === "organization" ||
    raw === "owner_private"
  ) {
    return raw as SkillMarketAudience
  }
  // Derive from v1 visibility/publisherType when the backend hasn't been
  // upgraded (pre-T03). T01/T03 adds `audience` to all responses.
  if (visibility === "public" && publisherType === "official")
    return "global_market"
  if (visibility === "private") return "owner_private"
  return "organization"
}

function mapVersionV1ToV2(v: SkillMarketVersion): SkillMarketV2Version {
  const raw = v as unknown as Record<string, unknown>
  const status = (() => {
    const s = raw["status"] as string | undefined
    if (s === "artifact_pending") return "artifact_pending" as const
    if (s === "failed") return "failed" as const
    return "ready" as const
  })()
  return {
    id: v.id,
    version: v.version,
    changelog: v.changelog ?? null,
    status,
    fileCount: v.fileCount,
    artifactSize: (raw["artifact_size"] as number | undefined) ?? v.packageSize,
    rawSize: v.packageSize,
    artifactSha256: (raw["artifact_sha256"] as string | undefined) ?? null,
    dependencies: v.dependencies,
    releasedAt: v.createdAt,
    failureCode: (raw["failure_code"] as string | undefined) ?? null,
  }
}

function mapItemV1ToV2(item: SkillMarketItem): SkillMarketV2Item {
  const raw = item as unknown as Record<string, unknown>
  const installedVersion = item.installedVersion ?? null
  const installState = installedVersion
    ? compareSemVer(item.currentVersion.version, installedVersion) > 0
      ? "update_available"
      : "installed"
    : "not_installed"
  return {
    id: item.id,
    slug: item.slug,
    displayName: item.displayName,
    summary: item.summary,
    category: item.category,
    iconUrl: item.iconUrl,
    tags: item.tags,
    audience: mapAudience(raw["audience"], item.visibility, item.publisherType),
    distributionPolicy: ((raw["distribution_policy"] as string | undefined) ===
    "mandatory"
      ? "mandatory"
      : "optional") as SkillMarketDistributionPolicy,
    publisher: item.publisherType,
    packageType: ((raw["package_type"] as string | undefined) ?? "skill") as
      | "skill"
      | "expert",
    currentVersion: mapVersionV1ToV2(item.currentVersion),
    compatibility: "unknown" as const,
    installState,
    installedVersion,
    canManage: item.canManage,
    organizationName: (raw["organization_name"] as string | undefined) ?? null,
    updatedAt: item.updatedAt,
  }
}

function buildFileTree(
  files: Array<{
    path: string
    size: number
    sha256: string
    mimeType: string | null
  }>
): SkillMarketV2FileNode[] {
  const root: Map<string, SkillMarketV2FileNode> = new Map()
  for (const file of files) {
    const parts = file.path.split("/")
    if (parts.length === 1) {
      root.set(file.path, {
        path: file.path,
        name: parts[0],
        size: file.size,
        directory: false,
        sha256: file.sha256,
      })
    } else {
      const dirName = parts[0]
      let dir = root.get(dirName)
      if (!dir) {
        dir = {
          path: dirName,
          name: dirName,
          size: 0,
          directory: true,
          children: [],
        }
        root.set(dirName, dir)
      }
      const _rest = parts.slice(1).join("/")
      // _rest unused; kept for clarity of the path structure
      void _rest
      ;(dir.children ??= []).push({
        path: file.path,
        name: parts[parts.length - 1],
        size: file.size,
        directory: false,
        sha256: file.sha256,
      })
    }
  }
  return Array.from(root.values()).sort((a, b) =>
    a.directory === b.directory
      ? a.name.localeCompare(b.name)
      : a.directory
        ? -1
        : 1
  )
}

// ---------------------------------------------------------------------------
// Transport-backed source — uses the existing v1 Tauri/web-server commands
// and maps responses to the v2 display model. The backend (T01/T03) already
// returns `audience`, `distribution_policy`, `artifact_sha256` etc. in the
// JSON; the TypeScript v1 types just didn't declare them.
// ---------------------------------------------------------------------------

class TransportSkillMarketSource implements SkillMarketSource {
  async list(query: SkillMarketListQueryV2): Promise<SkillMarketV2CatalogPage> {
    const view = (() => {
      if (query.view === "needs_update" || query.view === "installed")
        return "market" as const
      if (query.view === "organization") return "market" as const
      return query.view as "market" | "mine"
    })()
    // Decode cursor as a 1-based page number (base-64 encoded "page:N").
    let page = 1
    if (query.cursor) {
      try {
        const decoded = atob(query.cursor)
        const match = /^page:(\d+)$/.exec(decoded)
        if (match) page = parseInt(match[1], 10)
      } catch {
        // invalid cursor → start from page 1
      }
    }
    const params: SkillMarketListParams = {
      view,
      category: query.category ?? undefined,
      q: query.q || undefined,
      page,
      pageSize: query.limit,
      publisherType: query.publisher === "all" ? "all" : query.publisher,
    }
    const localInstallView =
      query.view === "installed" || query.view === "needs_update"
    const result = localInstallView
      ? await listCompleteCatalog(params)
      : await skillMarketList(params)
    const mapped = result.items.map(mapItemV1ToV2)
    const items = mapped.filter((item) => {
      if (query.view === "installed") return item.installedVersion !== null
      if (query.view === "needs_update") {
        return item.installState === "update_available"
      }
      return true
    })
    const hasMore = !localInstallView && page * query.limit < result.total
    const nextCursor = hasMore ? btoa(`page:${page + 1}`) : null
    return {
      items,
      nextCursor,
      total: localInstallView ? items.length : result.total,
      catalogRevision: "1",
      offline: false,
    }
  }

  async categories(): Promise<SkillMarketCategory[]> {
    return skillMarketCategories()
  }

  async detail(
    id: string,
    version?: string | null
  ): Promise<SkillMarketV2Detail> {
    const d = await skillMarketDetail(id, version)
    // SkillMarketDetail extends SkillMarketItem — fields are flat on `d`
    const base = mapItemV1ToV2(d as SkillMarketItem)
    return {
      ...base,
      files: buildFileTree(d.files),
      installTargets: d.installTargets ?? [],
      ownership: { source: "market" as const, managed: d.ownedByMe },
      compatibilityDetail: {
        minClientVersion: null,
        osArch: null,
        reason: null,
        deadline: null,
      },
    }
  }

  async versions(id: string): Promise<SkillMarketV2Version[]> {
    const vs = await skillMarketListVersions(id)
    return vs.map(mapVersionV1ToV2)
  }

  async files(id: string, version: string): Promise<SkillMarketV2FileNode[]> {
    const d = await skillMarketDetail(id, version)
    return buildFileTree(d.files)
  }

  async resolve(
    id: string,
    version: string
  ): Promise<SkillMarketInstallPlanV2> {
    // Build a lightweight v2 install plan from the skill detail. The actual
    // download uses the existing `skill_market_install` Tauri command — the
    // plan's ticketEndpoint signals which command the installer should call.
    const d = await skillMarketDetail(id, version)
    // SkillMarketDetail extends SkillMarketItem — fields are flat on `d`
    const v = d.currentVersion
    const raw = v as unknown as Record<string, unknown>
    const itemRaw = d as unknown as Record<string, unknown>
    const planItem: SkillMarketInstallPlanItemV2 = {
      skillId: d.id,
      versionId: v.id,
      artifactId: (raw["active_artifact_id"] as string | undefined) ?? v.id,
      slug: d.slug,
      displayName: d.displayName,
      version: v.version,
      audience: mapAudience(itemRaw["audience"], d.visibility, d.publisherType),
      distributionPolicy: ((itemRaw["distribution_policy"] as
        | string
        | undefined) === "mandatory"
        ? "mandatory"
        : "optional") as SkillMarketDistributionPolicy,
      artifactSize:
        (raw["artifact_size"] as number | undefined) ?? v.packageSize,
      artifactSha256: (raw["artifact_sha256"] as string | undefined) ?? "",
      signature: null,
      ticketEndpoint: "skill_market_install",
      dependencies: v.dependencies,
    }
    return {
      planId: `${id}:${version}:${Date.now()}`,
      catalogRevision: "1",
      targetSkillId: id,
      targetVersion: version,
      items: [planItem],
      totalBytes: planItem.artifactSize,
      dependencyCount: v.dependencies.length,
      mandatory: false,
    }
  }

  async publish(
    request: SkillMarketPublishRequestV2
  ): Promise<SkillMarketV2Item> {
    const v1Request: SkillMarketPublishRequest = {
      ...request,
      visibility:
        request.audience === "global_market"
          ? "public"
          : request.audience === "organization"
            ? "public"
            : "private",
    }
    const detail = await skillMarketPublish(v1Request)
    return mapItemV1ToV2(detail as unknown as SkillMarketItem)
  }

  async addVersion(
    request: SkillMarketAddVersionRequestV2
  ): Promise<SkillMarketV2Item> {
    const detail = await skillMarketAddVersion(request)
    return mapItemV1ToV2(detail as unknown as SkillMarketItem)
  }

  async updateMetadata(
    request: SkillMarketMetadataRequestV2
  ): Promise<SkillMarketV2Item> {
    const detail = await skillMarketUpdateMetadata({
      id: request.id,
      displayName: request.displayName,
      summary: request.summary,
      category: request.category,
      iconUrl: request.iconUrl,
      tags: request.tags,
      visibility:
        request.audience === "global_market"
          ? "public"
          : request.audience === "organization"
            ? "public"
            : "private",
    })
    return mapItemV1ToV2(detail as unknown as SkillMarketItem)
  }

  async delete(id: string): Promise<void> {
    await skillMarketDelete(id)
  }

  async uninstall(id: string): Promise<void> {
    await getTransport().call("skill_market_uninstall", { id })
  }

  async rebuildArtifact(
    id: string,
    version: string
  ): Promise<SkillMarketV2Version> {
    const v = await getTransport().call<SkillMarketVersion>(
      "skill_market_rebuild_artifact",
      { id, version }
    )
    return mapVersionV1ToV2(v)
  }
}

async function listCompleteCatalog(
  params: SkillMarketListParams
): Promise<Awaited<ReturnType<typeof skillMarketList>>> {
  const pageSize = 50
  const first = await skillMarketList({ ...params, page: 1, pageSize })
  const items = [...first.items]
  const pages = Math.ceil(first.total / Math.max(1, first.pageSize))
  for (let page = 2; page <= pages; page += 1) {
    const next = await skillMarketList({ ...params, page, pageSize })
    items.push(...next.items)
  }
  return { ...first, items, page: 1, pageSize: items.length || pageSize }
}

/**
 * Returns the active data source. The transport-backed implementation is the
 * default; pass `perfCount` to select the fixture source for list benchmarks.
 */
export function getSkillMarketSource(
  options?: SkillMarketSourceOptions
): SkillMarketSource {
  // Use fixture only when explicitly requested (e.g. ?perf=500 benchmarks).
  if (options?.perfCount) {
    return createFixtureSkillMarketSource(options)
  }
  return new TransportSkillMarketSource()
}
