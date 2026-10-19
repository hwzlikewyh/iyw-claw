"use client"

import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react"
import { useRuntimeTasks } from "@/hooks/use-runtime-tasks"
import type { ConnectionStatus } from "@/lib/types"
import type { RuntimeTask } from "@/lib/runtime-tasks"

interface TaskInspection {
  task: RuntimeTask
  onMessage?: (task: RuntimeTask) => void
  onCancelLoop?: (task: RuntimeTask) => void
}

type TaskState = ReturnType<typeof useRuntimeTasks>
interface PresentationState extends TaskState {
  contextKey?: string
  connectionId: string | null
  conversationId: number | null
  status: ConnectionStatus | null
  awaitingInput: boolean
  inspection: TaskInspection | null
  inspectTask: (selection: TaskInspection) => void
  closeInspection: () => void
}
const PresentationContext = createContext<PresentationState | null>(null)

export function ConversationPresentationProvider({
  children,
  contextKey,
  connectionId = null,
  conversationId = null,
  status,
  awaitingInput,
}: {
  children: ReactNode
  contextKey?: string
  connectionId?: string | null
  conversationId?: number | null
  status: ConnectionStatus | null
  awaitingInput: boolean
}) {
  const persistedId =
    conversationId != null && conversationId > 0 ? conversationId : null
  const state = useRuntimeTasks(connectionId, persistedId)
  const inspection = useTaskInspection(
    JSON.stringify([contextKey, connectionId, conversationId])
  )
  const value = useMemo(
    () => ({
      ...state,
      contextKey,
      connectionId,
      conversationId,
      status,
      awaitingInput,
      ...inspection,
    }),
    [
      state,
      contextKey,
      connectionId,
      conversationId,
      status,
      awaitingInput,
      inspection,
    ]
  )
  return (
    <PresentationContext.Provider value={value}>
      {children}
    </PresentationContext.Provider>
  )
}

function useTaskInspection(scope: string) {
  const [selected, setSelected] = useState<{
    scope: string
    value: TaskInspection
  } | null>(null)
  const inspectTask = useCallback(
    (value: TaskInspection) => setSelected({ scope, value }),
    [scope]
  )
  const closeInspection = useCallback(() => setSelected(null), [])
  return useMemo(
    () => ({
      inspection: selected?.scope === scope ? selected.value : null,
      inspectTask,
      closeInspection,
    }),
    [selected, scope, inspectTask, closeInspection]
  )
}

export function useConversationPresentation() {
  return useContext(PresentationContext)
}
