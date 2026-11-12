import { useEffect, useMemo, useState } from "react"
import {
  mcpListMarketplaces,
  mcpScanLocal,
  mcpSearchMarketplace,
} from "@/lib/api"
import { getSkillMarketSource } from "@/lib/skill-market-source"
import { toErrorMessage } from "@/lib/app-error"
import type { McpMarketplaceItem } from "@/lib/types"

export type CatalogEntry =
  | {
      key: string
      kind: "skill" | "plugin"
      name: string
      source: "official" | "user" | "organization"
      item: import("@/lib/skill-market").SkillMarketV2Item
    }
  | {
      key: string
      kind: "connector"
      name: string
      source: string
      providerName: string
      installed: boolean
      item: McpMarketplaceItem
    }
const CONNECTOR_BATCH = 50

async function connectorEntries(
  current: () => boolean
): Promise<CatalogEntry[]> {
  const [providers, local] = await Promise.all([
    mcpListMarketplaces(),
    mcpScanLocal(),
  ])
  if (!providers.length) throw Error("Connector catalog is unavailable")
  const installed = new Set(local.map((item) => item.id))
  const entries: CatalogEntry[] = []
  for (const provider of providers) {
    let offset = 0
    const seen = new Set<string>()
    while (current()) {
      const page = await mcpSearchMarketplace({
        providerId: provider.id,
        offset,
        limit: CONNECTOR_BATCH,
      })
      const fresh = page.filter((item) => !seen.has(item.server_id))
      if (page.length && !fresh.length)
        throw Error("Connector catalog pagination repeated")
      for (const item of fresh) {
        seen.add(item.server_id)
        entries.push({
          key: `connector:${provider.id}:${item.server_id}`,
          kind: "connector",
          name: item.name,
          source: `provider:${provider.id}`,
          providerName: provider.name,
          installed: installed.has(item.server_id),
          item,
        })
      }
      if (page.length < CONNECTOR_BATCH) break
      offset += page.length
    }
    if (!current()) break
  }
  return entries
}

async function skillEntries(): Promise<CatalogEntry[]> {
  const result = await getSkillMarketSource().list({
    view: "all",
    publisher: "all",
    distribution: "all",
    compatibility: "all",
    category: null,
    q: "",
    sort: "name",
    cursor: null,
    limit: Number.MAX_SAFE_INTEGER,
  })
  return result.items.map((item) => ({
    key: `${item.packageType}:${item.id}`,
    kind: item.packageType === "plugin" ? "plugin" : "skill",
    name: item.displayName,
    source: item.audience === "organization" ? "organization" : item.publisher,
    item,
  }))
}

export function useCapabilityCatalog() {
  const [revision, setRevision] = useState(0)
  const [state, setState] = useState({
    revision: -1,
    items: [] as CatalogEntry[],
    error: "",
  })
  useEffect(() => {
    let active = true
    void Promise.all([skillEntries(), connectorEntries(() => active)])
      .then(([skills, connectors]) => {
        if (!active) return
        const items = [
          ...new Map(
            [...skills, ...connectors].map((item) => [item.key, item])
          ).values(),
        ].sort(
          (a, b) => a.name.localeCompare(b.name) || a.key.localeCompare(b.key)
        )
        setState({ revision, items, error: "" })
      })
      .catch((cause) => {
        if (active)
          setState({ revision, items: [], error: toErrorMessage(cause) })
      })
    return () => {
      active = false
    }
  }, [revision])
  const items = useMemo(
    () => (state.revision === revision ? state.items : []),
    [state, revision]
  )
  return {
    items,
    loading: state.revision !== revision,
    error: state.revision === revision ? state.error : "",
    refresh: () => setRevision((value) => value + 1),
  }
}
