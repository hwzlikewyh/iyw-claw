"use client"

import {
  useEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react"
import { ImageSequenceControls } from "@/components/image-editor/image-sequence-controls"
import { useGalleryWheel } from "@/components/image-editor/use-gallery-wheel"

function useCarouselPosition(
  ref: RefObject<HTMLDivElement | null>,
  children: ReactNode
) {
  const [position, setPosition] = useState({ index: 0, total: 1, step: 1 })
  useEffect(() => {
    const element = ref.current
    const row = element?.firstElementChild
    if (!element || !row) return
    const update = () => {
      const first = row.firstElementChild as HTMLElement | null
      const gap = Number.parseFloat(getComputedStyle(row).columnGap) || 0
      const step = (first?.offsetWidth ?? element.clientWidth) + gap
      if (step <= 0) return
      const max = Math.max(0, element.scrollWidth - element.clientWidth)
      const total = Math.ceil(max / step) + 1
      const index =
        element.scrollLeft >= max - 1
          ? total - 1
          : Math.floor(element.scrollLeft / step)
      setPosition({ index, total, step })
    }
    const observer = new ResizeObserver(update)
    observer.observe(element)
    observer.observe(row)
    element.addEventListener("scroll", update)
    update()
    return () => {
      observer.disconnect()
      element.removeEventListener("scroll", update)
    }
  }, [children, ref])
  return position
}

export function ArtifactCarousel({ children }: { children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null)
  const position = useCarouselPosition(ref, children)
  const navigation = {
    ...position,
    onIndexChange: (index: number) => {
      ref.current?.scrollTo({
        left: index * position.step,
        behavior: "instant",
      })
    },
  }
  const wheelRef = useGalleryWheel(navigation)
  return (
    <div ref={wheelRef} className="min-w-0">
      <div className="flex justify-end">
        <ImageSequenceControls navigation={navigation} counter={false} />
      </div>
      <div
        ref={ref}
        className="max-w-full overflow-x-auto overscroll-x-contain pb-1"
      >
        {children}
      </div>
    </div>
  )
}
