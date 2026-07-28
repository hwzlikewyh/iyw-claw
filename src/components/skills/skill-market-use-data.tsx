"use client"

import { useCallback, useRef, useState } from "react"
import {
  useInstalledRemoteDetails,
  useMarketCategories,
  useMarketDetail,
  useMarketVersions,
} from "@/components/skills/skill-market-data-detail"
import {
  useMarketFilters,
  useMarketListing,
} from "@/components/skills/skill-market-data-list"
import type { SkillMarketSection } from "@/components/skills/skill-market-toolbar"
import type { SkillMarketItem } from "@/lib/skill-market"
import type { AgentSkillItem } from "@/lib/types"

function useMarketSelection() {
  const [selectedId, setSelectedIdState] = useState<string | null>(null)
  const [selectedVersion, setSelectedVersionState] = useState<string | null>(
    null
  )
  const selectedIdRef = useRef<string | null>(null)
  const selectedVersionRef = useRef<string | null>(null)
  const detailRequest = useRef(0)
  const versionsRequest = useRef(0)
  const setSelectedId = useCallback((next: string | null) => {
    if (next !== selectedIdRef.current) {
      detailRequest.current += 1
      versionsRequest.current += 1
      selectedIdRef.current = next
    }
    setSelectedIdState(next)
  }, [])
  const setSelectedVersion = useCallback((next: string | null) => {
    if (next !== selectedVersionRef.current) {
      detailRequest.current += 1
      selectedVersionRef.current = next
    }
    setSelectedVersionState(next)
  }, [])
  const getSelectedId = useCallback(() => selectedIdRef.current, [])
  return {
    selectedId,
    selectedVersion,
    detailRequest,
    versionsRequest,
    setSelectedId,
    setSelectedVersion,
    getSelectedId,
  }
}

export function useSkillMarketData(
  section: SkillMarketSection,
  installedSkills: AgentSkillItem[]
) {
  const categories = useMarketCategories()
  const filters = useMarketFilters()
  const selection = useMarketSelection()
  const listing = useMarketListing({
    section,
    filters,
    getSelectedId: selection.getSelectedId,
    setSelectedId: selection.setSelectedId,
    setSelectedVersion: selection.setSelectedVersion,
  })
  const details = useMarketDetail({
    selectedId: selection.selectedId,
    selectedVersion: selection.selectedVersion,
    refreshKey: listing.refreshKey,
    request: selection.detailRequest,
  })
  const versions = useMarketVersions({
    selectedId: selection.selectedId,
    refreshKey: listing.refreshKey,
    request: selection.versionsRequest,
  })
  const remoteById = useInstalledRemoteDetails(
    installedSkills,
    listing.refreshKey
  )
  const selectItem = (item: SkillMarketItem) => {
    selection.setSelectedId(item.id)
    selection.setSelectedVersion(null)
  }
  return {
    categories,
    ...listing.state,
    ...filters,
    selectedId: selection.selectedId,
    ...details,
    ...versions,
    remoteById,
    setSelectedId: selection.setSelectedId,
    setSelectedVersion: selection.setSelectedVersion,
    selectItem,
  }
}
