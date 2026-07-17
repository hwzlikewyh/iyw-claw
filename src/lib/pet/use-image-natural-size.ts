import { useEffect, useState } from "react"

export interface ImageNaturalSize {
  width: number
  height: number
}

interface MeasuredState {
  url: string | null
  size: ImageNaturalSize | null
}

export function useImageNaturalSize(
  url: string | null | undefined
): ImageNaturalSize | null {
  const [state, setState] = useState<MeasuredState>({ url: null, size: null })

  useEffect(() => {
    if (!url) return
    let cancelled = false
    const image = new Image()
    const apply = () => {
      if (cancelled) return
      const { naturalWidth: width, naturalHeight: height } = image
      setState({
        url,
        size: width > 0 && height > 0 ? { width, height } : null,
      })
    }
    image.onload = apply
    image.onerror = () => {
      if (!cancelled) setState({ url, size: null })
    }
    image.src = url
    if (image.complete) queueMicrotask(apply)

    return () => {
      cancelled = true
      image.onload = null
      image.onerror = null
    }
  }, [url])

  return state.url === url ? state.size : null
}
