"use client"

import { useCallback, useEffect, useState } from "react"
import type { Transport } from "@/lib/transport"
import { knowledgeError, type KnowledgeCategory } from "@/lib/cloud-knowledge"
import { getLibraryFolders, type LibraryFolders } from "@/lib/knowledge-library"

export function useLibraryFolders(
  transport: Transport,
  category: KnowledgeCategory
) {
  const [data, setData] = useState<LibraryFolders | null>(null)
  const [error, setError] = useState("")
  const [loadedKey, setLoadedKey] = useState("")
  const [revision, setRevision] = useState(0)
  const reload = useCallback(() => setRevision((value) => value + 1), [])
  useEffect(() => {
    let active = true
    const key = `${category}:${revision}`
    getLibraryFolders(transport, category)
      .then((next) => {
        if (active) setData(next)
        if (active) setLoadedKey(key)
      })
      .catch((reason) => {
        if (active) setError(knowledgeError(reason))
        if (active) setLoadedKey(key)
      })
      .finally(() => {
        if (active) setLoadedKey(key)
      })
    return () => {
      active = false
    }
  }, [transport, category, revision])
  return {
    data,
    error,
    busy: loadedKey !== `${category}:${revision}`,
    reload,
    revision,
  }
}
