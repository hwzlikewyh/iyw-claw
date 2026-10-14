"use client"

import { createContext, useContext } from "react"
import type { UIValue } from "@/lib/intelligent-ui/engine/expression"
import type { ResolvedNode } from "@/lib/intelligent-ui/engine/block"

export interface RenderContext {
  state: Readonly<Record<string, UIValue>>
  values: Readonly<Record<string, UIValue>>
  stateNames: ReadonlySet<string>
  update: (patch: Record<string, UIValue> | null) => void
  streaming: boolean
  render: (node: ResolvedNode) => React.ReactNode
}
export const UiRenderContext = createContext<RenderContext | null>(null)
export const NodePathContext = createContext("root")
export function useUiRender() {
  const value = useContext(UiRenderContext)
  if (!value) throw new Error("UI renderer context missing")
  return value
}
export const text = (value: unknown): string =>
  value == null
    ? "—"
    : typeof value === "object"
      ? JSON.stringify(value)
      : String(value)
export function useViewState<T extends UIValue>(
  field: string,
  initial: T
): [T, (value: T) => void] {
  const path = useContext(NodePathContext)
  const { state, update } = useUiRender()
  const name = `__view_${path.replace(/\./g, "_") || "root"}_${field}`
  return [(state[name] ?? initial) as T, (value) => update({ [name]: value })]
}
export const numeric = (value: unknown, fallback = 0): number =>
  typeof value === "number" && Number.isFinite(value) ? value : fallback
export const record = (value: unknown): Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {}
export const list = (value: unknown): unknown[] =>
  Array.isArray(value) ? value : []
export const toneClass = (tone: unknown) =>
  ({
    positive: "text-green-700 dark:text-green-400",
    negative: "text-destructive",
    warning: "text-amber-700 dark:text-amber-400",
    muted: "text-muted-foreground",
    info: "text-primary",
  })[String(tone)] ?? "text-foreground"
