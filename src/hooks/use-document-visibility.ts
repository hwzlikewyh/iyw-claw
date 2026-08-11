"use client"

import { useEffect, useState } from "react"

function isDocumentVisible(): boolean {
  return (
    typeof document === "undefined" || document.visibilityState === "visible"
  )
}

export function useDocumentVisibility(): boolean {
  const [visible, setVisible] = useState(isDocumentVisible)

  useEffect(() => {
    const updateVisibility = () => setVisible(isDocumentVisible())
    updateVisibility()
    document.addEventListener("visibilitychange", updateVisibility)
    return () => {
      document.removeEventListener("visibilitychange", updateVisibility)
    }
  }, [])

  return visible
}
