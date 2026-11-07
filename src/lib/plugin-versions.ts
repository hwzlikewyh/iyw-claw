import { getTransport } from "@/lib/transport"

export interface PluginVersionList {
  slug: string
  currentVersion: string
  versions: Array<{
    version: string
    recordedAt: string | null
    current: boolean
    available: boolean
    issue: string | null
  }>
  nextPage: number | null
}
export interface PluginVersionReview {
  related: Array<{
    id: string
    slug: string
    packageType: "skill" | "expert" | "plugin"
  }>
  slug: string
  currentVersion: string
  version: string
  state: "ready" | "blocked" | "applied"
  reviewToken: string | null
  scopeChanged: boolean
  permissionsChanged: boolean
  preparationChanged: boolean
  listed: boolean | null
  issues: string[]
  dependencies: Array<{
    packageType: "skill" | "expert" | "plugin"
    id: string
    slug: string
    version: string
    installedVersion: string | null
    changing: boolean
    managed: boolean
    ready: boolean
  }>
}
export const listPluginVersions = (
  id: string,
  page: number,
  transport = getTransport()
) =>
  transport.call<PluginVersionList>(
    "plugin_versions_list",
    { input: { marketSkillId: id, page } },
    { timeoutMs: 60_000 }
  )
export const inspectPluginVersion = (
  id: string,
  version: string,
  transport = getTransport()
) =>
  transport.call<PluginVersionReview>(
    "plugin_version_inspect",
    { input: { marketSkillId: id, version } },
    { timeoutMs: 300_000 }
  )
export const applyPluginVersion = (
  id: string,
  review: PluginVersionReview,
  transport = getTransport()
) =>
  transport.call<PluginVersionReview>(
    "plugin_version_apply",
    {
      input: {
        marketSkillId: id,
        version: review.version,
        reviewToken: review.reviewToken,
      },
    },
    { timeoutMs: 700_000 }
  )
