"use client"

import { useLayoutEffect, useRef, useState } from "react"
import { createModelLoader } from "./model-loader"
import { createModelScene, disposeModel } from "./model-scene"
import { acquirePreviewSlot } from "./preview-task-slots"

export function useModelPreview(src: string, visible: boolean) {
  const hostRef = useRef<HTMLDivElement>(null)
  const sceneRef = useRef<ReturnType<typeof createModelScene> | null>(null)
  const [state, setState] = useState({
    status: "loading",
    hasAnimations: false,
  })
  // OrbitControls 必须在画布脱离 document 前移除文档级监听。
  useLayoutEffect(() => {
    const host = hostRef.current
    if (!host || !visible) return
    const controller = new AbortController()
    const loader = createModelLoader(src)
    let scene: ReturnType<typeof createModelScene> | null = null
    let release: (() => void) | undefined
    const dispose = () => {
      loader.dispose()
      scene?.dispose()
      if (sceneRef.current === scene) sceneRef.current = null
      release?.()
    }
    void acquirePreviewSlot(controller.signal)
      .then(async (slot) => {
        release = slot
        if (controller.signal.aborted) {
          slot()
          return
        }
        setState({ status: "loading", hasAnimations: false })
        scene = createModelScene(host)
        sceneRef.current = scene
        const model = await loader.load(controller.signal)
        if (controller.signal.aborted) {
          for (const scene of model.scenes) disposeModel(scene)
          return
        }
        scene.setModel(model)
        setState({
          status: "ready",
          hasAnimations: model.animations.length > 0,
        })
      })
      .catch((error: unknown) => {
        dispose()
        if (!controller.signal.aborted) {
          console.warn("[preview] model load failed", {
            errorType: error instanceof Error ? error.name : "unknown",
          })
          setState({ status: "error", hasAnimations: false })
        }
      })
    return () => {
      controller.abort()
      dispose()
    }
  }, [src, visible])
  return { hostRef, sceneRef, ...state }
}
