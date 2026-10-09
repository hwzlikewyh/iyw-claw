"use client"

import {
  useCallback,
  useState,
  useSyncExternalStore,
  type SetStateAction,
} from "react"
import { getTransport, onTransportChange } from "@/lib/transport"

let hostGeneration = 0
onTransportChange(() => {
  hostGeneration += 1
})
const currentGeneration = () => hostGeneration
export const computerHostGeneration = currentGeneration

export function useComputerTransport() {
  return useSyncExternalStore(onTransportChange, getTransport, getTransport)
}

/** 切换宿主立即隐藏旧状态，并拒绝旧宿主的迟到结果。 */
export function useComputerHostState<T>(initial: T) {
  const transport = useComputerTransport()
  const generation = useSyncExternalStore(
    onTransportChange,
    currentGeneration,
    currentGeneration
  )
  const [defaultValue] = useState(() => initial)
  const [snapshot, setSnapshot] = useState({ generation, value: initial })
  const setValue = useCallback(
    (action: SetStateAction<T>) => {
      if (transport !== getTransport() || generation !== hostGeneration) return
      setSnapshot((previous) => {
        if (generation !== hostGeneration) return previous
        const current =
          previous.generation === generation ? previous.value : defaultValue
        const value =
          typeof action === "function"
            ? (action as (value: T) => T)(current)
            : action
        return { generation, value }
      })
    },
    [transport, generation, defaultValue]
  )
  return [
    snapshot.generation === generation ? snapshot.value : defaultValue,
    setValue,
  ] as const
}
