import { getTransport } from "@/lib/transport"

export interface PluginRemovalReview {
  id: string
  slug: string
  version: string
  state: "ready" | "removed" | "cleanup_pending" | "recovery_required"
  reviewToken: string | null
  affected: Array<{ id: string; slug: string }>
  retained: Array<{ id: string; slug: string; shared: boolean }>
  warnings: string[]
  clearAvailable: boolean
  parameterRecords: number
  legacyRecordsRetained: boolean
}
export type RemovalTransport = ReturnType<typeof getTransport>
export const inspectPluginRemoval = (id: string, transport = getTransport()) =>
  transport.call<PluginRemovalReview>(
    "plugin_remove_inspect",
    { input: { marketSkillId: id } },
    { timeoutMs: 180_000 }
  )
export const applyPluginRemoval = (
  id: string,
  reviewToken: string,
  clearAccountData: boolean,
  transport = getTransport()
) =>
  transport.call<PluginRemovalReview>(
    "plugin_remove_apply",
    { input: { marketSkillId: id, reviewToken, clearAccountData } },
    { timeoutMs: 700_000 }
  )
export const readPluginRemoval = (id: string, transport = getTransport()) =>
  transport.call<PluginRemovalReview | null>("plugin_remove_status", {
    input: { marketSkillId: id },
  })
export const retryPluginRemovalCleanup = (
  id: string,
  transport = getTransport()
) =>
  transport.call<PluginRemovalReview>(
    "plugin_remove_cleanup",
    { input: { marketSkillId: id } },
    { timeoutMs: 700_000 }
  )

export const recoverPluginRemoval = (id: string, transport = getTransport()) =>
  transport.call<PluginRemovalReview>(
    "plugin_remove_recover",
    { input: { marketSkillId: id } },
    { timeoutMs: 700_000 }
  )
