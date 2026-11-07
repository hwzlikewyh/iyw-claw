"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { getTransport } from "@/lib/transport"
import { extractAppCommandError } from "@/lib/app-error"
import {
  inspectPluginRemoval,
  applyPluginRemoval,
  readPluginRemoval,
  retryPluginRemovalCleanup,
  recoverPluginRemoval,
  type PluginRemovalReview,
} from "@/lib/plugin-remove"

interface State {
  view: PluginRemovalReview | null
  busy: boolean
  committing: boolean
  error: string
}
export function usePluginRemove(
  id: string,
  history: boolean,
  callbacks: { onRemoved: () => void; onRecovered: (version: string) => void }
) {
  const model = useModel()
  const inspect = useInspect(model, id, history)
  return {
    ...model.state,
    inspect,
    commit: (clear: boolean) => commit(model, id, clear, callbacks),
  }
}
function useModel() {
  const [transport] = useState(getTransport)
  const [state, setState] = useState<State>({
    view: null,
    busy: false,
    committing: false,
    error: "",
  })
  const lifecycle = useRef({
    alive: true,
    generation: 0,
    running: false,
    notified: false,
  })
  const update = useCallback(
    (patch: Partial<State>) => setState((value) => ({ ...value, ...patch })),
    []
  )
  return { transport, state, lifecycle, update }
}
type Model = ReturnType<typeof useModel>
function useInspect(
  { transport, lifecycle, update }: Model,
  id: string,
  history: boolean
) {
  const inspect = useCallback(async () => {
    if (lifecycle.current.running) return
    const generation = ++lifecycle.current.generation
    update({ busy: true, error: "", view: null })
    try {
      const view = history
        ? await readPluginRemoval(id, transport)
        : await inspectPluginRemoval(id, transport)
      if (current(lifecycle, generation)) update({ view })
    } catch (cause) {
      if (current(lifecycle, generation)) update({ error: errorText(cause) })
    } finally {
      if (current(lifecycle, generation)) update({ busy: false })
    }
  }, [id, history, transport, lifecycle, update])
  useEffect(() => {
    const active = lifecycle.current
    active.alive = true
    void inspect()
    return () => {
      active.alive = false
    }
  }, [inspect, lifecycle])
  return inspect
}
async function commit(
  model: Model,
  id: string,
  clear: boolean,
  callbacks: { onRemoved: () => void; onRecovered: (version: string) => void }
) {
  const { state, lifecycle, transport, update } = model,
    active = lifecycle.current,
    view = state.view
  if (
    active.running ||
    !view ||
    (view.state === "ready" && (!view.reviewToken || state.error))
  )
    return
  active.running = true
  update({ busy: true, committing: true, error: "" })
  try {
    const result =
      view.state === "cleanup_pending"
        ? await retryPluginRemovalCleanup(id, transport)
        : view.state === "recovery_required"
          ? await recoverPluginRemoval(id, transport)
          : await applyPluginRemoval(id, view.reviewToken!, clear, transport)
    if (!active.alive) return
    update({ view: result })
    if (
      ["removed", "cleanup_pending"].includes(result.state) &&
      !active.notified
    ) {
      active.notified = true
      callbacks.onRemoved()
    }
    if (view.state === "recovery_required" && result.state === "ready")
      callbacks.onRecovered(result.version)
  } catch (cause) {
    if (active.alive) update({ error: errorText(cause) })
  } finally {
    active.running = false
    if (active.alive) update({ busy: false, committing: false })
  }
}
const current = (lifecycle: Model["lifecycle"], generation: number) =>
  lifecycle.current.alive && lifecycle.current.generation === generation
const errorText = (cause: unknown) =>
  extractAppCommandError(cause)?.message || String(cause)
