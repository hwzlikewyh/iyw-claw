import { useEffect, useRef } from "react"

export function usePluginPanelDialog(visible: boolean, fullscreen: boolean) {
  const dialog = useRef<HTMLDialogElement>(null)
  const modal = useRef(false)
  useEffect(() => {
    const root = dialog.current
    if (!root) return
    if (root.open && (!visible || modal.current !== fullscreen)) root.close()
    if (visible && !root.open) {
      if (fullscreen) root.showModal()
      else root.show()
    }
    modal.current = fullscreen
  }, [visible, fullscreen])
  return dialog
}
