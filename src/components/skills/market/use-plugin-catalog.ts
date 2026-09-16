"use client"

import { useCallback, useEffect, useState } from "react"
import { toErrorMessage } from "@/lib/app-error"
import type { SkillMarketV2Item } from "@/lib/skill-market"
import { getSkillMarketSource } from "@/lib/skill-market-source"

async function loadPluginItems(view: "market" | "organization") {
  const result = await getSkillMarketSource().list({
    view,
    publisher: "all",
    distribution: "all",
    compatibility: "all",
    category: null,
    q: "",
    sort: "updated",
    cursor: null,
    // 适配器按接口页大小读取目录；筛选插件前保留整个受众范围。
    limit: Number.MAX_SAFE_INTEGER,
  })
  return result.items
    .filter((item) => item.packageType === "plugin")
    .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))
}

export function usePluginCatalog(view: "market" | "organization") {
  const [revision, setRevision] = useState(0)
  const [state, setState] = useState({
    view,
    revision,
    items: [] as SkillMarketV2Item[],
    loading: true,
    error: null as string | null,
  })
  const refresh = useCallback(() => setRevision((value) => value + 1), [])

  useEffect(() => {
    let cancelled = false
    void loadPluginItems(view)
      .then((items) => {
        if (cancelled) return
        setState({ view, revision, items, loading: false, error: null })
      })
      .catch((reason: unknown) => {
        if (cancelled) return
        const error = toErrorMessage(reason)
        console.error("[plugin-market] catalog load failed", { view, error })
        setState({ view, revision, items: [], loading: false, error })
      })
    return () => {
      cancelled = true
    }
  }, [revision, view])

  return {
    ...(state.view === view && state.revision === revision
      ? state
      : { view, items: [], loading: true, error: null }),
    refresh,
  }
}
