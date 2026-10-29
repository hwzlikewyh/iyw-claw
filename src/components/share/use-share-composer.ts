"use client"

import { useMemo, useState } from "react"
import type { AgentType } from "@/lib/types"
import {
  appendFiles,
  selectedSnapshot,
  toggleSelection,
  useShareSource,
} from "./share-composer-source"
import { useSharePublication } from "./share-composer-publication"

export function useShareComposer(conversationId: number, agentType: AgentType) {
  const sourceState = useShareSource(conversationId, agentType)
  const { source, ids, setIDs, setSource } = sourceState
  const [fileIDs, setFileIDs] = useState(new Set<string>())
  const candidates = useMemo(
    () => source?.candidates.filter((file) => ids.has(file.messageId)) ?? [],
    [source, ids]
  )
  const selected = useMemo(
    () => candidates.filter((file) => fileIDs.has(file.reference)),
    [candidates, fileIDs]
  )
  const snapshot = useMemo(
    () => selectedSnapshot(source, ids, selected),
    [source, ids, selected]
  )
  const publication = useSharePublication({
    conversationId,
    snapshot,
    files: selected,
  })
  const all = source !== null && ids.size === source.snapshot.messages.length
  const fallbackTitle = source?.title ?? ""
  return {
    ...publication,
    source,
    ids,
    fileIDs,
    snapshot,
    candidates,
    all,
    title: publication.title ?? fallbackTitle,
    error: publication.error || sourceState.loadError,
    retry: sourceState.retry,
    toggle: (id: string) => setIDs((current) => toggleSelection(current, id)),
    toggleFile: (id: string) =>
      setFileIDs((current) => toggleSelection(current, id)),
    toggleAll: () =>
      setIDs(
        new Set(
          all ? [] : source?.snapshot.messages.map((message) => message.id)
        )
      ),
    publish: () => publication.publish(fallbackTitle),
    addFiles: (files: FileList) => {
      const messageID = snapshot?.messages[snapshot.messages.length - 1]?.id
      if (!source || !messageID) return
      const addition = appendFiles({ files, source, messageID })
      setSource(addition.source)
      setFileIDs(
        (current) =>
          new Set([
            ...current,
            ...addition.additions.map((file) => file.reference),
          ])
      )
    },
  }
}
