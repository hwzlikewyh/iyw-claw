import { useRef, useState } from "react"
import type { NativePageState, NativePageDownload } from "@/lib/native-page-api"
import type { PageCapture } from "@/lib/page-capture"

export type NativePageProps = {
  id: string
  url: string
  visible: boolean
  tabId: string
  onNewTab: (url: string) => void
}

export function useNativePageModel() {
  const surface = useRef<HTMLDivElement>(null)
  const [state, setState] = useState<NativePageState | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [picking, setPicking] = useState(false)
  const [mobile, setMobile] = useState(false)
  const [zoom, setZoom] = useState(1)
  const [find, setFind] = useState<string | null>(null)
  const [revision, setRevision] = useState(0)
  const [capture, setCapture] = useState<{
    image: PageCapture
    crop: boolean
    tabId: string
  } | null>(null)
  const [errors, setErrors] = useState<string[]>([])
  const [downloads, setDownloads] = useState<NativePageDownload[]>([])
  return {
    surface,
    state,
    setState,
    error,
    setError,
    busy,
    setBusy,
    picking,
    setPicking,
    mobile,
    setMobile,
    zoom,
    setZoom,
    find,
    setFind,
    revision,
    setRevision,
    capture,
    setCapture,
    errors,
    setErrors,
    downloads,
    setDownloads,
  }
}

export type NativePageModel = ReturnType<typeof useNativePageModel>
