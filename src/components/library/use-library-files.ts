"use client"

import { useEffect, useState } from "react"
import type { Transport } from "@/lib/transport"
import { knowledgeError } from "@/lib/cloud-knowledge"
import {
  getLibraryFiles,
  isFileProcessing,
  refreshLibraryFiles,
  type LibraryFilePage,
  type LibraryTarget,
} from "@/lib/knowledge-library"

const STATUS_POLL_MS = 5000
const STATUS_BATCH_SIZE = 10

export interface LibraryFilesRequest {
  transport: Transport
  target: LibraryTarget
  page: number
  searchText: string
  revision: number
}

export function useLibraryFiles(request: LibraryFilesRequest) {
  const { transport, target, page, searchText, revision } = request
  const { scope, category, folderId } = target
  const [data, setData] = useState<LibraryFilePage>({ list: [], total: 0 })
  const [error, setError] = useState("")
  const [loadedKey, setLoadedKey] = useState("")
  const requestKey = `${scope}:${category}:${folderId}:${page}:${searchText}:${revision}`
  useEffect(() => {
    let active = true
    getLibraryFiles(transport, {
      scope,
      category,
      folderId,
      folderName: "",
      page,
      searchText,
    })
      .then((next) => {
        if (active) setData(next)
        if (active) setLoadedKey(requestKey)
      })
      .catch((reason) => {
        if (active) setError(knowledgeError(reason))
        if (active) setLoadedKey(requestKey)
      })
      .finally(() => {
        if (active) setLoadedKey(requestKey)
      })
    return () => {
      active = false
    }
  }, [
    transport,
    scope,
    category,
    folderId,
    page,
    searchText,
    revision,
    requestKey,
  ])
  const busy = loadedKey !== requestKey
  const [pollError, setPollError] = useState("")
  useProcessingRefresh({ transport, target, data, setData, setPollError, busy })
  return { data, error, pollError, busy }
}

type RefreshRequest = Pick<LibraryFilesRequest, "transport" | "target"> & {
  data: LibraryFilePage
  setData: React.Dispatch<React.SetStateAction<LibraryFilePage>>
  setPollError: React.Dispatch<React.SetStateAction<string>>
  busy: boolean
}

function useProcessingRefresh(request: RefreshRequest) {
  const { transport, target, data, setData, setPollError, busy } = request
  const { scope, category, folderId } = target
  const ids = data.list
    .filter((file) => isFileProcessing(file.docProcessStatus))
    .map((file) => file.fileId)
    .join(",")
  useEffect(() => {
    if (!ids || busy) return
    let active = true,
      pending = false,
      offset = 0
    const fileIds = ids.split(",")
    const timer = window.setInterval(async () => {
      if (document.hidden || pending) return
      pending = true
      const batch = fileIds.slice(offset, offset + STATUS_BATCH_SIZE)
      offset = (offset + STATUS_BATCH_SIZE) % fileIds.length
      try {
        const next = await refreshLibraryFiles(transport, {
          scope,
          category,
          fileIds: batch,
          forceRefresh: false,
        })
        if (!active) return
        const changes = new Map(next.list.map((file) => [file.fileId, file]))
        setData((previous) => ({
          ...previous,
          list: previous.list.map((file) => ({
            ...file,
            ...changes.get(file.fileId),
          })),
        }))
        setPollError("")
      } catch (reason) {
        if (active) setPollError(knowledgeError(reason))
      } finally {
        pending = false
      }
    }, STATUS_POLL_MS)
    return () => {
      active = false
      window.clearInterval(timer)
    }
  }, [transport, scope, category, folderId, ids, setData, setPollError, busy])
}
