"use client"

import { useEffect, useState } from "react"

import { correctUserMemory } from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import type { UserMemoryDocumentId } from "@/lib/user-memory-documents"
import {
  forgetUserMemory,
  listUserMemoryEntries,
  setUserMemoryEntryStatus,
  type UserMemoryEntry,
  type UserMemoryEntryPage,
  type ForgetUserMemoryResult,
} from "@/lib/user-memory-entries"

const SEARCH_DELAY_MS = 250
export const MEMORY_ENTRY_PAGE_SIZE = 100

function useMemoryEntryResponse(
  request: Parameters<typeof listUserMemoryEntries>[0],
  generation: number
) {
  const { document, query, includeInactive, offset } = request
  const [response, setResponse] = useState<{
    key: string
    page: UserMemoryEntryPage | null
    error: string | null
  } | null>(null)
  const requestKey = JSON.stringify([
    document,
    query,
    includeInactive,
    offset,
    generation,
  ])
  useEffect(() => {
    let current = true
    const timer = setTimeout(() => {
      listUserMemoryEntries({ document, query, includeInactive, offset })
        .then(
          (page) =>
            current && setResponse({ key: requestKey, page, error: null })
        )
        .catch(
          (reason) =>
            current &&
            setResponse({
              key: requestKey,
              page: null,
              error: toErrorMessage(reason),
            })
        )
    }, SEARCH_DELAY_MS)
    return () => {
      current = false
      clearTimeout(timer)
    }
  }, [document, query, includeInactive, offset, requestKey])
  return {
    page: response?.key === requestKey ? response.page : null,
    loading: response?.key !== requestKey,
    error: response?.key === requestKey ? response.error : null,
  }
}

export async function forgetMemoryEntry(
  state: ReturnType<typeof useUserMemoryEntries>,
  entry: UserMemoryEntry,
  purgeBackups: boolean
): Promise<ForgetUserMemoryResult | null> {
  if (!state.page || state.busy) return null
  state.setBusy(true)
  state.setError(null)
  try {
    const result = await forgetUserMemory({
      id: entry.id,
      document: state.document,
      expectedRevision: state.page.revision,
      purgeBackups,
      confirmation: "FORGET",
    })
    state.reload()
    return result
  } catch (error) {
    state.setError(toErrorMessage(error))
    throw error
  } finally {
    state.setBusy(false)
  }
}

export function useUserMemoryEntries(document: UserMemoryDocumentId) {
  const [query, setQuery] = useState("")
  const [includeInactive, setIncludeInactive] = useState(false)
  const [offset, setOffset] = useState(0)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [generation, setGeneration] = useState(0)
  const response = useMemoryEntryResponse(
    { document, query, includeInactive, offset },
    generation
  )
  const changeQuery = (value: string) => {
    setError(null)
    setOffset(0)
    setQuery(value)
  }
  const changeInactive = (value: boolean) => {
    setError(null)
    setOffset(0)
    setIncludeInactive(value)
  }
  const reload = () => setGeneration((value) => value + 1)
  return {
    document,
    query,
    includeInactive,
    offset,
    page: response.page,
    loading: response.loading,
    busy,
    error: error ?? response.error,
    setQuery: changeQuery,
    setIncludeInactive: changeInactive,
    setOffset,
    setBusy,
    setError,
    reload,
  }
}

export async function saveMemoryEntry(
  state: ReturnType<typeof useUserMemoryEntries>,
  entry: UserMemoryEntry,
  content: string
): Promise<boolean> {
  if (!state.page || state.busy) return false
  state.setBusy(true)
  state.setError(null)
  try {
    await correctUserMemory({
      document: state.document,
      oldContent: entry.content,
      newContent: content,
      expectedEtag: state.page.documentEtag,
    })
    state.reload()
    return true
  } catch (error) {
    state.setError(toErrorMessage(error))
    return false
  } finally {
    state.setBusy(false)
  }
}

export async function toggleMemoryEntry(
  state: ReturnType<typeof useUserMemoryEntries>,
  entry: UserMemoryEntry
): Promise<boolean> {
  if (!state.page || state.busy) return false
  state.setBusy(true)
  state.setError(null)
  try {
    await setUserMemoryEntryStatus({
      id: entry.id,
      document: state.document,
      expectedRevision: state.page.revision,
      active: !entry.active,
    })
    state.reload()
    return true
  } catch (error) {
    state.setError(toErrorMessage(error))
    return false
  } finally {
    state.setBusy(false)
  }
}
