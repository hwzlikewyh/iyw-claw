"use client"
import { createContext, useContext, type ReactNode } from "react"
const Context = createContext<
  ((text: string) => void | Promise<void>) | undefined
>(undefined)
export function UiActionsProvider({
  ask,
  children,
}: {
  ask?: (text: string) => void | Promise<void>
  children: ReactNode
}) {
  return <Context.Provider value={ask}>{children}</Context.Provider>
}
export const useUiActions = () => useContext(Context)
