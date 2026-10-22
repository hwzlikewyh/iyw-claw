"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import {
  getKnowledgeFiles,
  getKnowledgeFolders,
  knowledgeError,
  type KnowledgeCategory,
  type KnowledgeFile,
  type KnowledgeFolder,
} from "@/lib/cloud-knowledge"

export function useKnowledgeFolders(category: KnowledgeCategory) {
  const [folders, setFolders] = useState<KnowledgeFolder[] | null>(null)
  const [error, setError] = useState("")
  const [revision, setRevision] = useState(0)
  useEffect(() => {
    let active = true
    getKnowledgeFolders(category)
      .then((value) => {
        if (active) setFolders(value)
      })
      .catch((cause) => {
        if (active) setError(knowledgeError(cause))
      })
    return () => {
      active = false
    }
  }, [category, revision])
  const retry = () => {
    setFolders(null)
    setError("")
    setRevision((value) => value + 1)
  }
  return { folders, error, retry }
}

export function useKnowledgeFiles(
  category: KnowledgeCategory,
  folderId: string
) {
  const [files, setFiles] = useState<KnowledgeFile[]>([])
  const [total, setTotal] = useState(0)
  const [error, setError] = useState("")
  const [busy, setBusy] = useState(true)
  const nextPage = useRef(1)
  const active = useRef(false)
  const loading = useRef(false)
  const load = useCallback(async () => {
    if (loading.current) return
    loading.current = true
    setBusy(true)
    setError("")
    try {
      const value = await getKnowledgeFiles(
        category,
        folderId,
        nextPage.current
      )
      if (!active.current) return
      setFiles((previous) => [...previous, ...value.list])
      setTotal(value.total)
      nextPage.current++
    } catch (cause) {
      if (active.current) setError(knowledgeError(cause))
    } finally {
      loading.current = false
      if (active.current) setBusy(false)
    }
  }, [category, folderId])
  useEffect(() => {
    active.current = true
    void load()
    return () => {
      active.current = false
    }
  }, [load])
  return { files, total, error, busy, load }
}
