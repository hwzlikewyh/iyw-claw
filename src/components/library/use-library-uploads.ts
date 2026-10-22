"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import type { Transport } from "@/lib/transport"
import { knowledgeError } from "@/lib/cloud-knowledge"
import {
  LIBRARY_BATCH_LIMIT,
  type LibraryTarget,
} from "@/lib/knowledge-library"
import {
  releaseLibraryUpload,
  uploadLibraryFile,
  type LibraryUploadStatus,
} from "@/lib/knowledge-library-upload"
import {
  validateLibraryFile,
  type FileRuleFailure,
} from "@/lib/library-file-rules"
import { requireFileUploadCapability } from "@/lib/transport/capability-policy"

const UPLOAD_CONCURRENCY = 2

export interface UploadTask {
  id: string
  file: File
  phase: LibraryUploadStatus["phase"] | "ready" | "invalid"
  receivedBytes: number
  sentBytes: number
  error: string
}

export interface UploadQueueOptions {
  transport: Transport
  target: LibraryTarget
  onChanged: () => void
  ruleMessage: (failure: FileRuleFailure) => string
}

export function useLibraryUploads(options: UploadQueueOptions) {
  const [tasks, setTasks] = useState<UploadTask[]>([])
  const [busy, setBusy] = useState(false)
  const [validating, setValidating] = useState(false)
  const [error, setError] = useState("")
  const tasksRef = useRef(tasks)
  const controllersRef = useRef(new Map<string, AbortController>())
  const activeRef = useRef(true)
  const runningRef = useRef(false)
  const validationRef = useRef(0)
  const update = useCallback(
    (apply: (previous: UploadTask[]) => UploadTask[]) => {
      tasksRef.current = apply(tasksRef.current)
      if (activeRef.current) setTasks(tasksRef.current)
    },
    []
  )
  useUploadCleanup({
    ...options,
    tasksRef,
    controllersRef,
    activeRef,
    validationRef,
  })
  const state = {
    ...options,
    tasksRef,
    controllersRef,
    activeRef,
    runningRef,
    validationRef,
    update,
    setBusy,
    setValidating,
    setError,
  }
  return {
    tasks,
    busy,
    validating,
    error,
    addFiles: (files: File[]) => addFiles(state, files),
    start: () => startUploads(state),
    cancel: (id: string) => cancelTask(state, id),
    remove: (id: string) => removeTask(state, id),
    clear: () => clearQueue(state),
  }
}

type QueueState = UploadQueueOptions & {
  tasksRef: React.RefObject<UploadTask[]>
  runningRef: React.RefObject<boolean>
  validationRef: React.RefObject<number>
  activeRef: React.RefObject<boolean>
  controllersRef: React.RefObject<Map<string, AbortController>>
  update: (apply: (previous: UploadTask[]) => UploadTask[]) => void
  setBusy: (busy: boolean) => void
  setValidating: (busy: boolean) => void
  setError: (error: string) => void
}

async function addFiles(state: QueueState, files: File[]) {
  if (state.runningRef.current) return
  const generation = ++state.validationRef.current
  state.setValidating(true)
  state.setError("")
  const selected = [...state.tasksRef.current]
  let overflow = false
  for (const file of files) {
    if (
      selected.some(
        (task) =>
          task.file.name === file.name &&
          task.file.size === file.size &&
          task.file.lastModified === file.lastModified
      )
    )
      continue
    if (selected.length >= LIBRARY_BATCH_LIMIT) {
      overflow = true
      break
    }
    const failure = await validateLibraryFile(file)
    selected.push({
      id: crypto.randomUUID(),
      file,
      phase: failure ? "invalid" : "ready",
      receivedBytes: 0,
      sentBytes: 0,
      error: failure ? state.ruleMessage(failure) : "",
    })
  }
  if (!state.activeRef.current || state.validationRef.current !== generation)
    return
  state.update(() => selected)
  if (overflow)
    state.setError(
      state.ruleMessage({ key: "tooManyFiles", limit: LIBRARY_BATCH_LIMIT })
    )
  state.setValidating(false)
}

async function startUploads(state: QueueState) {
  if (state.runningRef.current) return
  state.runningRef.current = true
  state.setBusy(true)
  state.setError("")
  try {
    await requireFileUploadCapability()
    const pending = state.tasksRef.current.filter(
      (task) => task.phase === "ready" || task.phase === "failed"
    )
    let index = 0
    const worker = async () => {
      while (state.activeRef.current && index < pending.length) {
        const task = pending[index++]
        if (
          state.tasksRef.current.some(
            (row) => row.id === task.id && row.phase !== "cancelled"
          )
        )
          await runTask(state, task)
      }
    }
    await Promise.all(
      Array.from(
        { length: Math.min(UPLOAD_CONCURRENCY, pending.length) },
        worker
      )
    )
    if (state.activeRef.current) state.onChanged()
  } catch (reason) {
    if (state.activeRef.current) state.setError(knowledgeError(reason))
  } finally {
    state.runningRef.current = false
    if (state.activeRef.current) state.setBusy(false)
  }
}

async function runTask(state: QueueState, task: UploadTask) {
  const controller = new AbortController()
  state.controllersRef.current.set(task.id, controller)
  const patch = (change: Partial<UploadTask>) =>
    state.update((rows) =>
      rows.map((row) => (row.id === task.id ? { ...row, ...change } : row))
    )
  patch({ phase: "receiving", error: "" })
  try {
    await uploadLibraryFile({
      transport: state.transport,
      target: state.target,
      file: task.file,
      uploadId: task.id,
      signal: controller.signal,
      onStatus: (status) => {
        if (!controller.signal.aborted && state.activeRef.current)
          patch({
            phase: status.phase,
            receivedBytes: status.receivedBytes,
            sentBytes: status.sentBytes,
            error: status.error ?? "",
          })
      },
    })
    if (state.activeRef.current) state.onChanged()
  } catch (reason) {
    patch({
      phase: controller.signal.aborted ? "cancelled" : "failed",
      error: controller.signal.aborted ? "" : knowledgeError(reason),
    })
  } finally {
    state.controllersRef.current.delete(task.id)
  }
}

function cancelTask(state: QueueState, id: string) {
  const task = state.tasksRef.current.find((row) => row.id === id)
  if (!task || task.phase === "complete") return
  state.controllersRef.current.get(id)?.abort()
  state.update((rows) =>
    rows.map((row) =>
      row.id === id ? { ...row, phase: "cancelled", error: "" } : row
    )
  )
  void releaseLibraryUpload({ ...state, uploadId: id }).catch(() => {})
}

function removeTask(state: QueueState, id: string) {
  if (state.runningRef.current) return
  state.validationRef.current++
  state.update((rows) => rows.filter((row) => row.id !== id))
  void releaseLibraryUpload({ ...state, uploadId: id }).catch(() => {})
}

function clearQueue(state: QueueState) {
  if (state.runningRef.current) return
  state.validationRef.current++
  state.tasksRef.current.forEach((task) => {
    void releaseLibraryUpload({ ...state, uploadId: task.id }).catch(() => {})
  })
  state.update(() => [])
  state.setError("")
  state.setValidating(false)
}

function useUploadCleanup(
  state: Pick<
    QueueState,
    | "transport"
    | "target"
    | "tasksRef"
    | "controllersRef"
    | "activeRef"
    | "validationRef"
  >
) {
  const {
    transport,
    target,
    tasksRef,
    controllersRef,
    activeRef,
    validationRef,
  } = state
  useEffect(() => {
    activeRef.current = true
    const running = controllersRef.current
    const tasks = tasksRef.current
    return () => {
      activeRef.current = false
      // The generation is intentionally invalidated on unmount/target change.
      // eslint-disable-next-line react-hooks/exhaustive-deps
      validationRef.current++
      running.forEach((controller) => controller.abort())
      tasks.forEach((task) => {
        void releaseLibraryUpload({
          transport,
          target,
          uploadId: task.id,
        }).catch(() => {})
      })
    }
  }, [transport, target, tasksRef, controllersRef, activeRef, validationRef])
}
