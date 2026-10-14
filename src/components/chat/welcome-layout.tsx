"use client"

import { useLayoutEffect, useRef, type ReactNode } from "react"
import styles from "./welcome-layout.module.css"

function useWelcomeFit() {
  const viewportRef = useRef<HTMLDivElement>(null)
  const frameRef = useRef<HTMLDivElement>(null)
  const contentRef = useRef<HTMLDivElement>(null)

  useLayoutEffect(() => {
    const viewport = viewportRef.current
    const frame = frameRef.current
    const content = contentRef.current
    if (!viewport || !frame || !content) return

    let animationFrame = 0
    const fit = () => {
      const style = getComputedStyle(viewport)
      const availableHeight =
        viewport.clientHeight -
        parseFloat(style.paddingTop) -
        parseFloat(style.paddingBottom)
      const contentHeight = content.offsetHeight
      if (availableHeight <= 0 || contentHeight === 0) return

      // 先由 CSS 收紧间距，再按实际内容高度缩放，兼容分屏和界面缩放。
      const scale = Math.min(1, availableHeight / contentHeight)
      content.style.transform = scale < 1 ? `scale(${scale})` : "none"
      frame.style.height = `${contentHeight * scale}px`
    }
    const scheduleFit = () => {
      cancelAnimationFrame(animationFrame)
      animationFrame = requestAnimationFrame(fit)
    }
    const observer = new ResizeObserver(scheduleFit)
    observer.observe(viewport)
    observer.observe(content)
    fit()
    return () => {
      observer.disconnect()
      cancelAnimationFrame(animationFrame)
    }
  }, [])

  return { viewportRef, frameRef, contentRef }
}

export function WelcomeLayout({ children }: { children: ReactNode }) {
  const { viewportRef, frameRef, contentRef } = useWelcomeFit()
  return (
    <div ref={viewportRef} className={styles.viewport}>
      <div ref={frameRef} className={styles.frame}>
        <div ref={contentRef} className={styles.content}>
          {children}
        </div>
      </div>
    </div>
  )
}
