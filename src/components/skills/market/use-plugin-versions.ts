"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { getTransport } from "@/lib/transport"
import { extractAppCommandError } from "@/lib/app-error"
import {
  listPluginVersions,
  inspectPluginVersion,
  applyPluginVersion,
  type PluginVersionList,
  type PluginVersionReview,
} from "@/lib/plugin-versions"

interface State {
  list: PluginVersionList | null
  view: PluginVersionReview | null
  page: number
  loading: boolean
  busy: boolean
  applying: boolean
  error: string
}

export function usePluginVersions(id: string) {
  const model = useVersionModel(),
    load = useVersionLoad(model, id)
  return {
    ...model.state,
    load,
    inspect: (version: string) => execute(model, id, version),
    apply: () => execute(model, id),
  }
}

function useVersionModel() {
  const [transport] = useState(getTransport)
  const [state, setState] = useState<State>({
    list: null,
    view: null,
    page: 1,
    loading: false,
    busy: false,
    applying: false,
    error: "",
  })
  const lifecycle = useRef({ alive: true, running: false, generation: 0 })
  const update = useCallback(
    (patch: Partial<State>) =>
      setState((current) => ({ ...current, ...patch })),
    []
  )
  return { transport, state, lifecycle, update }
}
type Model = ReturnType<typeof useVersionModel>

function useVersionLoad({ transport, lifecycle, update }: Model, id: string) {
  const load = useCallback(
    async (page: number) => {
      if (lifecycle.current.running) return
      const generation = ++lifecycle.current.generation
      update({ loading: true, error: "", view: null })
      try {
        const list = await listPluginVersions(id, page, transport)
        if (current(lifecycle, generation)) update({ list, page })
      } catch (cause) {
        if (current(lifecycle, generation)) update({ error: errorText(cause) })
      } finally {
        if (current(lifecycle, generation)) update({ loading: false })
      }
    },
    [id, transport, lifecycle, update]
  )
  useEffect(() => {
    const active = lifecycle.current
    active.alive = true
    active.running = false
    update({ busy: false, applying: false })
    void load(1)
    return () => {
      active.alive = false
      active.running = false
    }
  }, [load, lifecycle, update])
  return load
}

async function execute(model: Model, id: string, version?: string) {
  const { state, transport, lifecycle, update } = model
  if (
    lifecycle.current.running ||
    (!version &&
      (state.view?.state !== "ready" || !state.view.reviewToken || state.error))
  )
    return
  lifecycle.current.running = true
  const generation = ++lifecycle.current.generation
  update({
    loading: false,
    busy: true,
    applying: !version,
    error: "",
    ...(version ? { view: null } : {}),
  })
  try {
    const view = version
      ? await inspectPluginVersion(id, version, transport)
      : await applyPluginVersion(id, state.view!, transport)
    if (current(lifecycle, generation)) update({ view })
  } catch (cause) {
    if (current(lifecycle, generation)) update({ error: errorText(cause) })
  } finally {
    if (lifecycle.current.generation === generation) {
      lifecycle.current.running = false
      if (lifecycle.current.alive) update({ busy: false, applying: false })
    }
  }
}

const current = (lifecycle: Model["lifecycle"], generation: number) =>
  lifecycle.current.alive && lifecycle.current.generation === generation
const errorText = (cause: unknown) =>
  extractAppCommandError(cause)?.message || String(cause)
