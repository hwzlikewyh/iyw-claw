"use client"

import { useCallback, useEffect, useRef, useState } from "react"

export const MAX_SCREENSHOTS = 5
const MAX_IMAGE_BYTES = 5 * 1024 * 1024
const MAX_TOTAL_BYTES = 20 * 1024 * 1024
export const SCREENSHOT_TYPES = "image/png,image/jpeg,image/webp"
const ALLOWED_TYPES = new Set(SCREENSHOT_TYPES.split(","))

export interface ReportScreenshot {
  id: string
  file: File
  url: string
}

function validFiles(current: ReportScreenshot[], added: File[]) {
  const total = [...current.map((image) => image.file), ...added]
  return (
    total.length <= MAX_SCREENSHOTS &&
    total.every(
      (file) =>
        ALLOWED_TYPES.has(file.type) &&
        file.size > 0 &&
        file.size <= MAX_IMAGE_BYTES
    ) &&
    total.reduce((size, file) => size + file.size, 0) <= MAX_TOTAL_BYTES
  )
}

export function useReportScreenshots() {
  const [images, setImages] = useState<ReportScreenshot[]>([])
  const current = useRef<ReportScreenshot[]>([])
  useEffect(
    () => () =>
      current.current.forEach((image) => URL.revokeObjectURL(image.url)),
    []
  )

  const add = useCallback((files: File[]) => {
    if (!validFiles(current.current, files)) return false
    const additions = files.map((file) => {
      const url = URL.createObjectURL(file)
      return { id: url, file, url }
    })
    current.current = [...current.current, ...additions]
    setImages(current.current)
    return true
  }, [])

  const remove = useCallback((id: string) => {
    const image = current.current.find((entry) => entry.id === id)
    if (image) URL.revokeObjectURL(image.url)
    current.current = current.current.filter((entry) => entry.id !== id)
    setImages(current.current)
  }, [])

  const clear = useCallback(() => {
    current.current.forEach((image) => URL.revokeObjectURL(image.url))
    current.current = []
    setImages([])
  }, [])

  return { images, add, remove, clear }
}
