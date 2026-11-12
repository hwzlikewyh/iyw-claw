"use client"

import { useLayoutEffect, useRef, type ReactNode } from "react"

export type AuxPanelSurfaceProps = {
  children: ReactNode
  desktopSlot: HTMLElement | null
  mobile: boolean
  open: boolean
  title: string
  onOpenChange: (open: boolean) => void
}

export function AuxPanelSurface(props: AuxPanelSurfaceProps) {
  const dialog = useRef<HTMLDialogElement>(null)
  const suspended = useRef<HTMLDialogElement[]>([])
  const { desktopSlot, mobile, open } = props
  useLayoutEffect(() => {
    const root = dialog.current
    if (!root) return
    const update = () =>
      positionSurface(root, { desktopSlot, mobile, open }, suspended.current)
    update()
    const observer = new ResizeObserver(update)
    if (desktopSlot) observer.observe(desktopSlot)
    window.addEventListener("resize", update)
    window.addEventListener("scroll", update, true)
    return () => {
      observer.disconnect()
      window.removeEventListener("resize", update)
      window.removeEventListener("scroll", update, true)
    }
  }, [desktopSlot, mobile, open])
  return (
    <dialog
      ref={dialog}
      aria-label={props.title}
      className="fixed z-30 m-0 min-h-0 max-h-none max-w-none flex-col overflow-visible border-0 bg-sidebar p-0 text-sidebar-foreground backdrop:bg-black/50"
      onCancel={(event) => {
        event.preventDefault()
        event.stopPropagation()
        props.onOpenChange(false)
      }}
    >
      {props.children}
    </dialog>
  )
}

function positionSurface(
  root: HTMLDialogElement,
  props: Pick<AuxPanelSurfaceProps, "desktopSlot" | "mobile" | "open">,
  suspended: HTMLDialogElement[]
) {
  const bounds = props.desktopSlot?.getBoundingClientRect()
  // 断点切换时桌面占位稍后挂载，先保留当前顶层顺序，避免丢失嵌套模态框。
  if (props.open && !props.mobile && !bounds) return
  const visible = props.open && (props.mobile || Boolean(bounds?.width))
  root.style.display = visible ? "flex" : "none"
  Object.assign(
    root.style,
    props.mobile
      ? {
          inset: "0",
          width: "100vw",
          height: "100dvh",
          padding:
            "env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left)",
        }
      : {
          inset: "auto",
          left: `${bounds?.left ?? 0}px`,
          top: `${bounds?.top ?? 0}px`,
          width: `${bounds?.width ?? 0}px`,
          height: `${bounds?.height ?? 0}px`,
          padding: "0",
        }
  )
  syncDialog(root, { visible, modal: props.mobile, suspended })
}

function syncDialog(
  root: HTMLDialogElement,
  {
    visible,
    modal,
    suspended,
  }: {
    visible: boolean
    modal: boolean
    suspended: HTMLDialogElement[]
  }
) {
  if (root.open && visible && root.matches(":modal") === modal) return
  const focused = document.activeElement
  const nested = [
    ...new Set([
      ...suspended,
      ...root.querySelectorAll<HTMLDialogElement>("dialog:modal"),
    ]),
  ].filter((dialog) => dialog.isConnected && dialog.style.display !== "none")
  suspended.splice(0, suspended.length, ...nested)
  for (const dialog of nested.reverse()) dialog.close()
  if (root.open) root.close()
  if (!visible) return
  if (modal) root.showModal()
  else root.show()
  for (const dialog of nested.reverse()) if (!dialog.open) dialog.showModal()
  suspended.length = 0
  if (focused instanceof HTMLElement && root.contains(focused))
    focused.focus({ preventScroll: true })
}
