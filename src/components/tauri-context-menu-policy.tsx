"use client"

import { useEffect } from "react"

const NATIVE_CONTEXT_MENU_ATTRIBUTE = "data-native-context-menu"
const NON_TEXT_INPUT_TYPES = new Set([
  "button",
  "checkbox",
  "color",
  "file",
  "hidden",
  "image",
  "radio",
  "range",
  "reset",
  "submit",
])

function isEditableTarget(target: Element) {
  const editable = target.closest("input, textarea, [contenteditable]")
  if (!editable) return false
  if (editable instanceof HTMLInputElement)
    return !NON_TEXT_INPUT_TYPES.has(editable.type)
  if (editable instanceof HTMLTextAreaElement) return true
  return editable instanceof HTMLElement && editable.isContentEditable
}

function explicitlyAllowsNativeContextMenu(target: EventTarget | null) {
  return (
    target instanceof Element &&
    Boolean(target.closest(`[${NATIVE_CONTEXT_MENU_ATTRIBUTE}]`))
  )
}

function allowsApplicationContextMenu(target: EventTarget | null) {
  if (!(target instanceof Element)) return false
  if (isEditableTarget(target)) return true

  // Radix opens its menu from this trigger and calls preventDefault itself.
  return Boolean(
    target.closest('[data-slot="context-menu-trigger"]:not([data-disabled])')
  )
}

export function TauriContextMenuPolicy() {
  useEffect(() => {
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
      return
    }

    const handleContextMenu = (event: MouseEvent) => {
      if (explicitlyAllowsNativeContextMenu(event.target)) {
        // An editable control can live inside a Radix trigger. Stop this event
        // before that ancestor replaces the requested native editing menu.
        event.stopPropagation()
        return
      }
      if (allowsApplicationContextMenu(event.target)) return
      event.preventDefault()
    }

    document.addEventListener("contextmenu", handleContextMenu, true)
    return () => {
      document.removeEventListener("contextmenu", handleContextMenu, true)
    }
  }, [])

  return null
}
