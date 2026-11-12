import { getTransport } from "@/lib/transport"

export interface PluginRecoveryReview {
  slug: string
  version: string
  state: "ready" | "blocked" | "recovered"
  scopeChanged: boolean
  resetPermissions: boolean
  credentialsRetained: boolean
  listed: boolean | null
  reviewToken: string | null
  issues: string[]
  dependencies: Array<{
    id: string
    slug: string
    requiredVersion: string
    installedVersion: string | null
    ready: boolean
  }>
}

export const inspectPluginRecovery = (
  marketSkillId: string,
  transport = getTransport()
) =>
  transport.call<PluginRecoveryReview>(
    "plugin_recovery_inspect",
    { input: { marketSkillId } },
    { timeoutMs: 180_000 }
  )
export const applyPluginRecovery = (
  marketSkillId: string,
  reviewToken: string,
  transport = getTransport()
) =>
  transport.call<PluginRecoveryReview>(
    "plugin_recovery_apply",
    { input: { marketSkillId, reviewToken } },
    { timeoutMs: 700_000 }
  )
