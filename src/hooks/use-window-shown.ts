"use client"

import { useSyncExternalStore } from "react"
import { getCurrentWindow } from "@/lib/platform"

type AppWindow = NonNullable<Awaited<ReturnType<typeof getCurrentWindow>>>

// 拖动缩放会连续触发事件，停顿后再查询一次窗口状态。
const QUERY_DELAY_MS = 150

const listeners = new Set<() => void>()
let shown = true
let failureReported = false
let release: (() => void) | null = null

function accept(next: boolean) {
  if (shown === next) return
  shown = next
  listeners.forEach((listener) => listener())
}

function reportFailure(error: unknown) {
  if (failureReported) return
  failureReported = true
  console.warn("[WindowShown] window state unavailable", { error })
}

async function queryShown(appWindow: AppWindow) {
  const [minimized, visible] = await Promise.all([
    appWindow.isMinimized(),
    appWindow.isVisible(),
  ])
  return visible && !minimized
}

function watchWindow() {
  let disposed = false
  let revision = 0
  let timer: ReturnType<typeof setTimeout> | undefined
  const unlisten: Array<() => void> = []
  const keep = (stop: () => void) => {
    if (disposed) stop()
    else unlisten.push(stop)
  }
  const refresh = async (appWindow: AppWindow) => {
    const current = ++revision
    try {
      const next = await queryShown(appWindow)
      if (!disposed && current === revision) accept(next)
    } catch (error) {
      // 查询失败时按可见处理，避免动效停在暂停状态。
      if (!disposed) accept(true)
      reportFailure(error)
    }
  }
  const start = async () => {
    const appWindow = await getCurrentWindow()
    if (!appWindow || disposed) return
    const schedule = () => {
      clearTimeout(timer)
      timer = setTimeout(() => void refresh(appWindow), QUERY_DELAY_MS)
    }
    // 最小化与还原触发 resize，隐藏到托盘与重新显示触发焦点变化。
    keep(await appWindow.onResized(schedule))
    keep(await appWindow.onFocusChanged(schedule))
    if (!disposed) await refresh(appWindow)
  }
  void start().catch(reportFailure)
  return () => {
    disposed = true
    clearTimeout(timer)
    unlisten.forEach((stop) => stop())
  }
}

function subscribe(listener: () => void) {
  listeners.add(listener)
  release ??= watchWindow()
  return () => {
    listeners.delete(listener)
    if (listeners.size > 0) return
    release?.()
    release = null
    shown = true
  }
}

const readShown = () => shown

/** 当前桌面窗口是否未最小化且未隐藏；Web 模式与查询失败时恒为 true。 */
export function useWindowShown(): boolean {
  return useSyncExternalStore(subscribe, readShown, () => true)
}
