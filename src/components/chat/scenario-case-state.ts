"use client"

import { useEffect, useRef, useState } from "react"
import {
  scenarioCaseDetail,
  scenarioCasesCatalog,
  type ScenarioCase,
  type ScenarioCaseDetail,
} from "@/lib/scenario-cases"
import type { ScenarioCategory } from "@/lib/types"
import type { ComposerInjectContent } from "./message-input"
import type { ScenarioPreferences } from "./scenario-preferences"
import { scenarioContent } from "./scenario-content"

export interface ScenarioCasesProps {
  category?: ScenarioCategory
  preferences: ScenarioPreferences
  refreshKey?: string | number
  onSelect: (payload: ComposerInjectContent) => boolean | void
}

function useCaseCatalog(refreshKey?: string | number) {
  const [items, setItems] = useState<ScenarioCase[]>([])
  useEffect(() => {
    let active = true
    let generation = 0
    const refresh = () => {
      const request = ++generation
      void scenarioCasesCatalog()
        .then((value) => {
          if (active && request === generation) setItems(value.items)
        })
        .catch(() => {
          if (active && request === generation) setItems([])
        })
    }
    refresh()
    window.addEventListener("focus", refresh)
    return () => {
      active = false
      window.removeEventListener("focus", refresh)
    }
  }, [refreshKey])
  return items
}

function useCaseDetail(id: string | null) {
  const [state, setState] = useState<{
    id: string
    value?: ScenarioCaseDetail
    failed?: boolean
  } | null>(null)
  const [attempt, setAttempt] = useState(0)
  useEffect(() => {
    if (!id || state?.id !== id || !state.value) return
    const expires = state.value.assets
      .map((asset) => Date.parse(asset.expiresAt))
      .filter(Number.isFinite)
    if (!expires.length) return
    const timer = setTimeout(
      () => setAttempt((value) => value + 1),
      Math.max(5000, Math.min(...expires) - Date.now() - 60000)
    )
    return () => clearTimeout(timer)
  }, [id, state])
  useEffect(() => {
    if (!id) return
    let active = true
    void scenarioCaseDetail(id)
      .then((value) => {
        if (active) setState({ id, value })
      })
      .catch(() => {
        if (active) setState({ id, failed: true })
      })
    return () => {
      active = false
    }
  }, [id, attempt])
  return {
    detail: state?.id === id ? state.value : undefined,
    failed: state?.id === id && state.failed,
    clear: () => setState(null),
    retry: () => {
      setState(null)
      setAttempt((value) => value + 1)
    },
  }
}

function useCaseDialog() {
  const [open, setOpen] = useState(false)
  const [selected, setSelected] = useState<ScenarioCase | null>(null)
  const makingSame = useRef(false)
  const clear = () => setSelected(null)
  return {
    open,
    selected,
    select: setSelected,
    close: () => {
      makingSame.current = true
      clear()
      setOpen(false)
    },
    changeOpen: (next: boolean) => {
      if (!next) clear()
      setOpen(next)
    },
    closeAutoFocus: (event: Event) => {
      if (makingSame.current) event.preventDefault()
      makingSame.current = false
    },
  }
}

export function useCaseController(props: ScenarioCasesProps) {
  const catalog = useCaseCatalog(props.refreshKey)
  const dialog = useCaseDialog()
  const request = useCaseDetail(
    dialog.open ? (dialog.selected?.id ?? null) : null
  )
  const items = catalog
    .filter((item) => item.categoryKey === props.category?.key)
    .filter((item) => !props.preferences[item.scenarioId]?.hidden)
    .sort((left, right) => left.sortOrder - right.sortOrder)
  const makeSame = () => {
    if (!request.detail) return
    const scenario = request.detail.scenario
    const accepted = props.onSelect({
      ...scenarioContent(
        scenario,
        props.preferences[scenario.id]?.promptOverride ||
          scenario.promptTemplate
      ),
      model: request.detail.model || undefined,
    })
    if (accepted !== false) dialog.close()
  }
  return {
    items,
    ...dialog,
    ...request,
    makeSame,
    title: `${props.category?.displayName ?? ""}相关案例`,
    select: (item: ScenarioCase) => {
      request.clear()
      dialog.select(item)
    },
    changeOpen: (next: boolean) => {
      if (!next) request.clear()
      dialog.changeOpen(next)
    },
  }
}

export type CaseController = ReturnType<typeof useCaseController>
