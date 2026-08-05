import type {
  SkillDependency,
  SkillDependencyInput,
  SkillMarketArtifactStatus,
  SkillMarketAudience,
  SkillMarketCategory,
  SkillMarketCompatibility,
  SkillMarketDistributionPolicy,
  SkillMarketInstallPlanItemV2,
  SkillMarketInstallState,
  SkillMarketListQueryV2,
  SkillMarketOwnershipSource,
  SkillMarketPublisher,
  SkillMarketSort,
  SkillMarketV2CatalogPage,
  SkillMarketV2CompatibilityDetail,
  SkillMarketV2FileNode,
  SkillMarketV2Item,
  SkillMarketV2Version,
} from "@/lib/skill-market"
import {
  SkillMarketSourceError,
  type SkillMarketAddVersionRequestV2,
  type SkillMarketMetadataRequestV2,
  type SkillMarketPublishRequestV2,
  type SkillMarketSource,
  type SkillMarketSourceOptions,
} from "@/lib/skill-market-source"
import type { AgentType } from "@/lib/types"

// ---------------------------------------------------------------------------
// Typed fixtures for the Skill Market v2 UI (Task 10).
// The backend contract v2 is not frozen yet, so every audience / distribution /
// artifact / compatibility / install state is covered by deterministic data
// here. Task 13 swaps the source when the frozen contract lands.
// ---------------------------------------------------------------------------

interface FixtureFlatFile {
  path: string
  size: number
  sha256: string | null
}

interface FixtureRecord {
  item: SkillMarketV2Item
  versions: SkillMarketV2Version[]
  flatFiles: Record<string, FixtureFlatFile[]>
  ownership: { source: SkillMarketOwnershipSource; managed: boolean }
  compatibilityDetail: SkillMarketV2CompatibilityDetail
  installTargets: AgentType[]
}

const NOW = "2026-08-01T04:00:00.000Z"

const CATEGORIES: SkillMarketCategory[] = [
  { key: "office-efficiency", fallbackName: "Office Efficiency", sortOrder: 1 },
  { key: "content-creation", fallbackName: "Content Creation", sortOrder: 2 },
  { key: "dev-programming", fallbackName: "Development", sortOrder: 3 },
  { key: "data-analysis", fallbackName: "Data Analysis", sortOrder: 4 },
  { key: "design-media", fallbackName: "Design & Media", sortOrder: 5 },
  { key: "ai-agent", fallbackName: "AI Agent", sortOrder: 6 },
  { key: "knowledge-management", fallbackName: "Knowledge", sortOrder: 7 },
  { key: "business-ops", fallbackName: "Business Ops", sortOrder: 8 },
  { key: "education", fallbackName: "Education", sortOrder: 9 },
  { key: "professional", fallbackName: "Professional", sortOrder: 10 },
  { key: "it-ops-security", fallbackName: "IT Ops & Security", sortOrder: 11 },
  { key: "life-service", fallbackName: "Life Service", sortOrder: 12 },
]

function version(
  id: string,
  versionValue: string,
  status: SkillMarketArtifactStatus,
  artifactSize: number,
  dependencies: SkillDependency[] = [],
  changelog: string | null = null
): SkillMarketV2Version {
  return {
    id,
    version: versionValue,
    changelog,
    status,
    fileCount: 0,
    artifactSize,
    rawSize: artifactSize,
    artifactSha256: status === "ready" ? `fixture-sha256-${id}` : null,
    dependencies,
    releasedAt: "2026-07-20T08:00:00.000Z",
    failureCode: status === "failed" ? "build_failed" : null,
  }
}

function toDependencies(input: SkillDependencyInput[]): SkillDependency[] {
  return input.map((d) => ({
    skillId: d.slug,
    slug: d.slug,
    version: d.version,
  }))
}

function flatFiles(
  prefix: string,
  count: number,
  large = false
): FixtureFlatFile[] {
  const files: FixtureFlatFile[] = []
  const size = large ? 48 * 1024 : 2 * 1024
  files.push({
    path: `${prefix}/SKILL.md`,
    size: 8 * 1024,
    sha256: `fixture-sha-${prefix}-skill`,
  })
  files.push({
    path: `${prefix}/agents/openai.yaml`,
    size: 3 * 1024,
    sha256: `fixture-sha-${prefix}-agents`,
  })
  for (let index = 1; index <= count; index += 1) {
    const group = Math.floor(index / 40)
    files.push({
      path: `${prefix}/scripts/group-${group}/step-${String(index).padStart(3, "0")}.py`,
      size: size + (index % 7) * 137,
      sha256: `fixture-sha-${prefix}-${index}`,
    })
  }
  return files
}

function buildFileTree(files: FixtureFlatFile[]): SkillMarketV2FileNode[] {
  const root: SkillMarketV2FileNode[] = []
  for (const file of files) {
    const parts = file.path.split("/")
    let node = root
    let prefix = ""
    for (let index = 0; index < parts.length; index += 1) {
      const part = parts[index]
      prefix = prefix ? `${prefix}/${part}` : part
      const last = index === parts.length - 1
      if (last) {
        node.push({
          path: file.path,
          name: part,
          size: file.size,
          directory: false,
          sha256: file.sha256,
        })
        continue
      }
      let child = node.find(
        (candidate) => candidate.directory && candidate.name === part
      )
      if (!child) {
        child = {
          path: prefix,
          name: part,
          size: 0,
          directory: true,
          children: [],
        }
        node.push(child)
      }
      node = child.children ?? []
    }
  }
  const sortNodes = (
    nodes: SkillMarketV2FileNode[]
  ): SkillMarketV2FileNode[] => {
    return nodes
      .slice()
      .sort((left, right) => {
        if (left.directory !== right.directory) return left.directory ? -1 : 1
        return left.name.localeCompare(right.name, "en")
      })
      .map((node) => {
        if (node.children) node.children = sortNodes(node.children)
        return node
      })
  }
  return sortNodes(root)
}

function makeItem(
  id: string,
  slug: string,
  displayName: string,
  summary: string,
  category: string,
  audience: SkillMarketAudience,
  publisher: SkillMarketPublisher,
  distributionPolicy: SkillMarketDistributionPolicy,
  compatibility: SkillMarketCompatibility,
  installState: SkillMarketInstallState,
  installedVersion: string | null,
  currentVersion: SkillMarketV2Version,
  options: {
    tags?: string[]
    iconUrl?: string | null
    canManage?: boolean
    organizationName?: string | null
    updatedAt?: string
    packageType?: "skill" | "expert"
  } = {}
): SkillMarketV2Item {
  return {
    id,
    slug,
    displayName,
    summary,
    category,
    iconUrl: options.iconUrl ?? null,
    tags: options.tags ?? [],
    audience,
    distributionPolicy,
    publisher,
    packageType: options.packageType ?? "skill",
    currentVersion,
    compatibility,
    installState,
    installedVersion,
    canManage: options.canManage ?? publisher === "user",
    organizationName: options.organizationName ?? null,
    updatedAt: options.updatedAt ?? NOW,
  }
}

function record(
  item: SkillMarketV2Item,
  versions: SkillMarketV2Version[],
  flatFileList: FixtureFlatFile[],
  ownership: FixtureRecord["ownership"] = { source: "market", managed: true },
  compatibilityDetail: SkillMarketV2CompatibilityDetail = {
    minClientVersion: null,
    osArch: null,
    reason: null,
    deadline: null,
  }
): FixtureRecord {
  const flatFilesByVersion: Record<string, FixtureFlatFile[]> = {}
  for (const v of versions) {
    flatFilesByVersion[v.version] = flatFileList
    v.fileCount = flatFileList.length
  }
  return {
    item,
    versions,
    flatFiles: flatFilesByVersion,
    ownership,
    compatibilityDetail,
    installTargets: item.installedVersion ? ["codex"] : [],
  }
}

function buildFixtureCatalog(): FixtureRecord[] {
  const ghFiles = flatFiles("gh-workflow", 12)
  const crmFiles = flatFiles("crm-agent", 24)
  const imageFiles = flatFiles("image-workflows", 9)
  const teamFiles = flatFiles("team-skill-alpha", 16)
  const hugeFiles = flatFiles("huge-docs", 480, true)

  const records: FixtureRecord[] = [
    record(
      makeItem(
        "skill-gh-workflow",
        "gh-workflow-expert",
        "GitHub Workflow Expert",
        "Plan, write and review GitHub Actions workflows with reproducible step templates.",
        "dev-programming",
        "global_market",
        "official",
        "optional",
        "compatible",
        "not_installed",
        null,
        version("ver-gh-110", "1.1.0", "ready", 14339),
        { tags: ["github", "actions"], packageType: "expert" }
      ),
      [
        version(
          "ver-gh-110",
          "1.1.0",
          "ready",
          14339,
          [],
          "Deterministic workflow ordering."
        ),
        version("ver-gh-100", "1.0.0", "ready", 13208),
      ],
      ghFiles
    ),
    record(
      makeItem(
        "skill-crm-agent",
        "crm-agent",
        "CRM Agent",
        "Interact with the CRM API: contacts, deals, pipeline stages and rollback-safe updates.",
        "business-ops",
        "global_market",
        "official",
        "optional",
        "compatible",
        "not_installed",
        null,
        version("ver-crm-230", "2.3.0", "ready", 48216),
        { tags: ["crm", "api"] }
      ),
      [
        version(
          "ver-crm-230",
          "2.3.0",
          "ready",
          48216,
          [],
          "Adds deal rollback."
        ),
      ],
      crmFiles
    ),
    record(
      makeItem(
        "skill-data-analyzer",
        "data-analyzer",
        "Data Analyzer",
        "CSV/Parquet analysis with distribution checks and report generation.",
        "data-analysis",
        "global_market",
        "official",
        "optional",
        "incompatible",
        "not_installed",
        null,
        version("ver-data-300", "3.0.0", "ready", 66420),
        { tags: ["data", "analysis"] }
      ),
      [version("ver-data-300", "3.0.0", "ready", 66420)],
      flatFiles("data-analyzer", 18),
      { source: "market", managed: true },
      {
        minClientVersion: "0.2.0",
        osArch: "win64 / arm64",
        reason: "Requires the reconciler fingerprint introduced in 0.2.0.",
        deadline: null,
      }
    ),
    record(
      makeItem(
        "skill-image-workflows",
        "image-workflows",
        "Image Workflows",
        "Batch image processing: resize, watermark, EXIF cleanup and asset exports.",
        "design-media",
        "global_market",
        "official",
        "mandatory",
        "compatible",
        "not_installed",
        null,
        version("ver-image-140", "1.4.0", "ready", 92140, [
          {
            skillId: "skill-gh-workflow",
            slug: "gh-workflow-expert",
            version: "1.1.0",
          },
        ]),
        { tags: ["image", "batch"], packageType: "expert" }
      ),
      [version("ver-image-140", "1.4.0", "ready", 92140)],
      imageFiles
    ),
    record(
      makeItem(
        "skill-sales-assistant",
        "sales-assistant",
        "Sales Assistant",
        "Sales pipeline helper for the Acme organization with lead scoring and follow-ups.",
        "business-ops",
        "organization",
        "official",
        "optional",
        "compatible",
        "not_installed",
        null,
        version("ver-sales-210", "2.1.0", "ready", 38840),
        { tags: ["sales", "acme"], organizationName: "Acme Org" }
      ),
      [version("ver-sales-210", "2.1.0", "ready", 38840)],
      flatFiles("sales-assistant", 14)
    ),
    record(
      makeItem(
        "skill-finance-report",
        "finance-report",
        "Finance Report",
        "Mandatory monthly finance report generator for the Acme organization.",
        "professional",
        "organization",
        "official",
        "mandatory",
        "compatible",
        "not_installed",
        null,
        version("ver-fin-150", "1.5.0", "ready", 27400),
        { tags: ["finance"], organizationName: "Acme Org" }
      ),
      [version("ver-fin-150", "1.5.0", "ready", 27400)],
      flatFiles("finance-report", 10)
    ),
    record(
      makeItem(
        "skill-my-notes",
        "my-notes",
        "My Notes",
        "Personal note-taking skill with templates, only visible to the owner.",
        "knowledge-management",
        "owner_private",
        "user",
        "optional",
        "compatible",
        "not_installed",
        null,
        version("ver-notes-010", "0.1.0", "ready", 8210),
        { tags: ["notes", "private"], canManage: true }
      ),
      [version("ver-notes-010", "0.1.0", "ready", 8210)],
      flatFiles("my-notes", 6),
      { source: "user_dir", managed: false }
    ),
    record(
      makeItem(
        "skill-my-script-pack",
        "my-script-pack",
        "My Script Pack",
        "Private shell scripting helpers. Artifact is still being built.",
        "dev-programming",
        "owner_private",
        "user",
        "optional",
        "compatible",
        "preparing",
        null,
        version("ver-script-030", "0.3.0", "artifact_pending", 0),
        { tags: ["scripts", "private"], canManage: true, packageType: "expert" }
      ),
      [version("ver-script-030", "0.3.0", "artifact_pending", 0)],
      flatFiles("my-script-pack", 8),
      { source: "user_dir", managed: false }
    ),
    record(
      makeItem(
        "skill-team-alpha",
        "team-skill-alpha",
        "Team Skill Alpha",
        "Organization-wide QA skill whose latest artifact build failed.",
        "it-ops-security",
        "organization",
        "user",
        "optional",
        "compatible",
        "not_installed",
        null,
        version("ver-team-040", "0.4.0", "failed", 0),
        { tags: ["qa"], organizationName: "Acme Org", canManage: true }
      ),
      [version("ver-team-040", "0.4.0", "failed", 0)],
      teamFiles
    ),
    record(
      makeItem(
        "skill-dev-toolkit",
        "dev-toolkit",
        "Dev Toolkit",
        "Local development helpers already installed at the latest version.",
        "dev-programming",
        "global_market",
        "official",
        "optional",
        "compatible",
        "installed",
        "1.0.0",
        version("ver-dev-100", "1.0.0", "ready", 19320),
        { tags: ["dev"] }
      ),
      [version("ver-dev-100", "1.0.0", "ready", 19320)],
      flatFiles("dev-toolkit", 11)
    ),
    record(
      makeItem(
        "skill-ops-runbook",
        "ops-runbook",
        "Ops Runbook",
        "Incident runbook with a new release available.",
        "it-ops-security",
        "global_market",
        "official",
        "optional",
        "compatible",
        "update_available",
        "1.0.0",
        version("ver-ops-120", "1.2.0", "ready", 26410),
        { tags: ["ops", "runbook"] }
      ),
      [
        version(
          "ver-ops-120",
          "1.2.0",
          "ready",
          26410,
          [],
          "Adds rollback checklist."
        ),
        version("ver-ops-110", "1.1.0", "ready", 25040),
        version("ver-ops-100", "1.0.0", "ready", 24100),
      ],
      flatFiles("ops-runbook", 13)
    ),
    record(
      makeItem(
        "skill-audit-companion",
        "audit-companion",
        "Audit Companion",
        "Mandatory compliance skill currently blocked by policy for this client.",
        "professional",
        "global_market",
        "official",
        "mandatory",
        "compatible",
        "blocked",
        "1.0.0",
        version("ver-audit-110", "1.1.0", "ready", 31200),
        { tags: ["compliance", "audit"] }
      ),
      [version("ver-audit-110", "1.1.0", "ready", 31200)],
      flatFiles("audit-companion", 15),
      { source: "market", managed: true },
      {
        minClientVersion: "0.1.9",
        osArch: "win64",
        reason: "Blocked until the rollout window closes for your channel.",
        deadline: "2026-08-15T00:00:00.000Z",
      }
    ),
    record(
      makeItem(
        "skill-market-basics",
        "market-basics",
        "Market Basics",
        "Introductory skill whose compatibility has not been verified yet.",
        "education",
        "global_market",
        "official",
        "optional",
        "unknown",
        "not_installed",
        null,
        version("ver-basics-010", "0.1.0", "ready", 7400),
        { tags: ["intro"] }
      ),
      [version("ver-basics-010", "0.1.0", "ready", 7400)],
      flatFiles("market-basics", 5)
    ),
    record(
      makeItem(
        "skill-huge-docs",
        "huge-docs",
        "Huge Docs",
        "Large documentation corpus used to exercise the virtualized file tree.",
        "knowledge-management",
        "global_market",
        "official",
        "optional",
        "compatible",
        "not_installed",
        null,
        version("ver-huge-010", "0.1.0", "ready", 28 * 1024 * 1024),
        { tags: ["docs", "large"] }
      ),
      [version("ver-huge-010", "0.1.0", "ready", 28 * 1024 * 1024)],
      hugeFiles
    ),
  ]
  return records
}

function generatePerfItems(count: number): FixtureRecord[] {
  const audiences: SkillMarketAudience[] = [
    "global_market",
    "organization",
    "owner_private",
  ]
  const states: SkillMarketInstallState[] = [
    "not_installed",
    "installed",
    "update_available",
    "blocked",
    "preparing",
  ]
  const records: FixtureRecord[] = []
  for (let index = 0; index < count; index += 1) {
    const id = `perf-${String(index).padStart(5, "0")}`
    const audience = audiences[index % audiences.length]
    const state = states[index % states.length]
    const installed =
      state === "not_installed" || state === "preparing" ? null : "1.0.0"
    const v = version(
      `perf-ver-${index}`,
      "1.0.0",
      "ready",
      9000 + (index % 97)
    )
    const item = makeItem(
      id,
      `perf-skill-${index}`,
      `Perf Skill ${index}`,
      `Synthetic catalog entry number ${index} for list benchmarks.`,
      CATEGORIES[index % CATEGORIES.length].key,
      audience,
      index % 3 === 0 ? "official" : "user",
      index % 5 === 0 ? "mandatory" : "optional",
      index % 7 === 0 ? "incompatible" : "compatible",
      state,
      installed,
      v,
      {
        tags: ["perf", String(index % 10)],
        organizationName: audience === "organization" ? "Acme Org" : null,
      }
    )
    records.push(record(item, [v], flatFiles(`perf/${id}`, 3)))
  }
  return records
}

function fixtureLatency(perf: boolean): Promise<void> {
  const ms = perf ? 8 : 140 + Math.random() * 180
  return new Promise((resolve) => window.setTimeout(resolve, ms))
}

function matchesQuery(
  item: SkillMarketV2Item,
  query: SkillMarketListQueryV2
): boolean {
  if (query.publisher !== "all" && item.publisher !== query.publisher)
    return false
  if (
    query.distribution !== "all" &&
    item.distributionPolicy !== query.distribution
  ) {
    return false
  }
  if (
    query.compatibility !== "all" &&
    item.compatibility !== query.compatibility
  ) {
    return false
  }
  if (query.category && item.category !== query.category) return false
  const needle = query.q.trim().toLowerCase()
  if (needle) {
    const haystack = [item.displayName, item.summary, item.slug, ...item.tags]
      .join(" ")
      .toLowerCase()
    if (!haystack.includes(needle)) return false
  }
  return true
}

function matchesView(
  item: SkillMarketV2Item,
  view: SkillMarketListQueryV2["view"]
): boolean {
  switch (view) {
    case "market":
      return item.audience === "global_market"
    case "organization":
      return item.audience === "organization"
    case "mine":
      return item.audience === "owner_private"
    case "installed":
      return item.installState !== "not_installed"
    case "needs_update":
      return item.installState === "update_available"
  }
}

const INSTALL_RANK: Record<SkillMarketInstallState, number> = {
  installed: 0,
  update_available: 1,
  blocked: 2,
  preparing: 3,
  not_installed: 4,
}

function sortItems(
  items: SkillMarketV2Item[],
  sort: SkillMarketSort
): SkillMarketV2Item[] {
  const copy = items.slice()
  switch (sort) {
    case "updated":
      return copy.sort((left, right) =>
        right.updatedAt.localeCompare(left.updatedAt)
      )
    case "name":
      return copy.sort((left, right) =>
        left.displayName.localeCompare(right.displayName, "en")
      )
    case "installed":
      return copy.sort(
        (left, right) =>
          INSTALL_RANK[left.installState] - INSTALL_RANK[right.installState]
      )
    case "recommended":
      return copy
  }
}

export function createFixtureSkillMarketSource(
  options: SkillMarketSourceOptions = {}
): SkillMarketSource {
  let records = options.perfCount
    ? generatePerfItems(options.perfCount)
    : buildFixtureCatalog()
  let revision = 0
  const catalogRevision = () => `fixture-${revision}`
  const bumpRevision = () => {
    revision += 1
  }
  const perf = Boolean(options.perfCount)

  const findRecord = (id: string): FixtureRecord => {
    const found = records.find((candidate) => candidate.item.id === id)
    if (!found)
      throw new SkillMarketSourceError("catalog_stale", `unknown skill ${id}`)
    return found
  }

  const toPage = (
    items: SkillMarketV2Item[],
    cursor: string | null,
    limit: number
  ): SkillMarketV2CatalogPage => {
    const safeLimit = Math.max(1, Math.min(200, limit))
    const offset = cursor ? Math.max(0, Number(cursor) || 0) : 0
    const pageItems = items.slice(offset, offset + safeLimit)
    const nextOffset = offset + pageItems.length
    return {
      items: pageItems,
      nextCursor: nextOffset < items.length ? String(nextOffset) : null,
      total: items.length,
      catalogRevision: catalogRevision(),
      offline: false,
    }
  }

  return {
    async list(query) {
      await fixtureLatency(perf)
      const filtered = records
        .map((entry) => entry.item)
        .filter(
          (item) => matchesView(item, query.view) && matchesQuery(item, query)
        )
      return toPage(sortItems(filtered, query.sort), query.cursor, query.limit)
    },

    async categories() {
      await fixtureLatency(perf)
      return CATEGORIES
    },

    async detail(id, versionValue) {
      await fixtureLatency(perf)
      const found = findRecord(id)
      const selectedVersion =
        versionValue && found.versions.some((v) => v.version === versionValue)
          ? versionValue
          : found.item.currentVersion.version
      const flatFileList = found.flatFiles[selectedVersion] ?? []
      return {
        ...found.item,
        files: buildFileTree(flatFileList),
        installTargets: found.installTargets,
        ownership: found.ownership,
        compatibilityDetail: found.compatibilityDetail,
      }
    },

    async versions(id) {
      await fixtureLatency(perf)
      return findRecord(id)
        .versions.slice()
        .sort((a, b) => b.version.localeCompare(a.version))
    },

    async files(id, versionValue) {
      await fixtureLatency(perf)
      const found = findRecord(id)
      return buildFileTree(found.flatFiles[versionValue] ?? [])
    },

    async resolve(id, versionValue) {
      await fixtureLatency(perf)
      const found = findRecord(id)
      const targetVersion =
        found.versions.find((v) => v.version === versionValue) ??
        found.item.currentVersion
      if (targetVersion.status === "artifact_pending") {
        throw new SkillMarketSourceError(
          "artifact_not_ready",
          `artifact for ${id}@${targetVersion.version} is still building`
        )
      }
      if (targetVersion.status === "failed") {
        throw new SkillMarketSourceError(
          "artifact_not_ready",
          `artifact for ${id}@${targetVersion.version} failed to build`
        )
      }
      if (found.item.compatibility !== "compatible") {
        throw new SkillMarketSourceError(
          "client_incompatible",
          `skill ${id} is incompatible with this client`
        )
      }
      if (found.item.installState === "blocked") {
        throw new SkillMarketSourceError(
          "version_blocked",
          `skill ${id} is blocked by policy`
        )
      }

      const planItems: SkillMarketInstallPlanItemV2[] = []
      const visit = (
        current: FixtureRecord,
        versionToInstall: SkillMarketV2Version
      ) => {
        planItems.push({
          skillId: current.item.id,
          versionId: versionToInstall.id,
          artifactId: versionToInstall.artifactSha256 ?? versionToInstall.id,
          slug: current.item.slug,
          displayName: current.item.displayName,
          version: versionToInstall.version,
          audience: current.item.audience,
          distributionPolicy: current.item.distributionPolicy,
          artifactSize: versionToInstall.artifactSize,
          artifactSha256: versionToInstall.artifactSha256 ?? "fixture-sha",
          signature: null,
          ticketEndpoint: "/skills/artifact-ticket",
          dependencies: versionToInstall.dependencies,
        })
        for (const dependency of versionToInstall.dependencies) {
          const depRecord = records.find(
            (candidate) => candidate.item.slug === dependency.slug
          )
          if (!depRecord) {
            throw new SkillMarketSourceError(
              "dependency_unavailable",
              `missing dependency ${dependency.slug}`
            )
          }
          const depVersion =
            depRecord.versions.find((v) => v.version === dependency.version) ??
            depRecord.item.currentVersion
          if (depVersion.status !== "ready") {
            throw new SkillMarketSourceError(
              "dependency_unavailable",
              `dependency ${dependency.slug} is not ready`
            )
          }
          visit(depRecord, depVersion)
        }
      }
      visit(found, targetVersion)

      return {
        planId: `plan-${id}-${targetVersion.version}-${catalogRevision()}`,
        catalogRevision: catalogRevision(),
        targetSkillId: id,
        targetVersion: targetVersion.version,
        items: planItems,
        totalBytes: planItems.reduce(
          (sum, planItem) => sum + planItem.artifactSize,
          0
        ),
        dependencyCount: Math.max(0, planItems.length - 1),
        mandatory: planItems.some(
          (planItem) => planItem.distributionPolicy === "mandatory"
        ),
      }
    },

    async publish(request: SkillMarketPublishRequestV2) {
      await fixtureLatency(perf)
      if (request.audience === "global_market") {
        throw new SkillMarketSourceError(
          "audience_denied",
          "global market publishing is backend-only"
        )
      }
      const id = `fixture-upload-${records.length + 1}`
      const artifactSize = request.files.reduce(
        (sum, file) => sum + file.size,
        0
      )
      const uploadedVersion = version(
        `${id}-v1`,
        request.version,
        "artifact_pending",
        artifactSize,
        toDependencies(request.dependencies),
        request.changelog || null
      )
      const item = makeItem(
        id,
        request.slug,
        request.displayName,
        request.summary,
        request.category,
        request.audience,
        "user",
        "optional",
        "compatible",
        "preparing",
        null,
        uploadedVersion,
        {
          tags: request.tags,
          iconUrl: request.iconUrl,
          canManage: true,
          organizationName:
            request.audience === "organization" ? "Acme Org" : null,
        }
      )
      const flatFileList: FixtureFlatFile[] = request.files.map((file) => ({
        path: file.path,
        size: file.size,
        sha256: null,
      }))
      records.unshift(
        record(item, [uploadedVersion], flatFileList, {
          source: "user_dir",
          managed: false,
        })
      )
      bumpRevision()
      return item
    },

    async addVersion(request: SkillMarketAddVersionRequestV2) {
      await fixtureLatency(perf)
      const found = findRecord(request.id)
      const artifactSize = request.files.reduce(
        (sum, file) => sum + file.size,
        0
      )
      const added = version(
        `${request.id}-v${found.versions.length + 1}`,
        request.version,
        "artifact_pending",
        artifactSize,
        toDependencies(request.dependencies),
        request.changelog || null
      )
      const flatFileList: FixtureFlatFile[] = request.files.map((file) => ({
        path: file.path,
        size: file.size,
        sha256: null,
      }))
      found.versions.unshift(added)
      found.flatFiles[added.version] = flatFileList
      added.fileCount = flatFileList.length
      found.item = {
        ...found.item,
        currentVersion: added,
        installState: "preparing",
        updatedAt: NOW,
      }
      bumpRevision()
      return found.item
    },

    async updateMetadata(request: SkillMarketMetadataRequestV2) {
      await fixtureLatency(perf)
      const found = findRecord(request.id)
      found.item = {
        ...found.item,
        displayName: request.displayName,
        summary: request.summary,
        category: request.category,
        iconUrl: request.iconUrl,
        tags: request.tags,
        audience: request.audience,
        updatedAt: NOW,
      }
      bumpRevision()
      return found.item
    },

    async delete(id) {
      await fixtureLatency(perf)
      const before = records.length
      records = records.filter((candidate) => candidate.item.id !== id)
      if (records.length === before) {
        throw new SkillMarketSourceError("catalog_stale", `unknown skill ${id}`)
      }
      bumpRevision()
    },

    async uninstall(id) {
      await fixtureLatency(perf)
      const found = findRecord(id)
      found.item = {
        ...found.item,
        installState: "not_installed",
        installedVersion: null,
        updatedAt: NOW,
      }
      bumpRevision()
    },

    async rebuildArtifact(id, versionValue) {
      await fixtureLatency(perf)
      const found = findRecord(id)
      const targetVersion =
        found.versions.find((v) => v.version === versionValue) ??
        found.item.currentVersion
      if (targetVersion.status === "ready") return targetVersion
      const rebuilt = {
        ...targetVersion,
        status: "ready" as const,
        artifactSize: targetVersion.rawSize ?? 10240,
        artifactSha256: `fixture-sha256-${targetVersion.id}-rebuilt`,
        failureCode: null,
        fileCount: (found.flatFiles[targetVersion.version] ?? []).length,
      }
      const index = found.versions.indexOf(targetVersion)
      if (index >= 0) found.versions[index] = rebuilt
      if (found.item.currentVersion.id === targetVersion.id) {
        found.item = {
          ...found.item,
          currentVersion: rebuilt,
          installState: "not_installed",
          installedVersion: null,
          updatedAt: NOW,
        }
      }
      bumpRevision()
      return rebuilt
    },
  }
}
