import { useCallback } from "react"

import { extractAppCommandError } from "@/lib/app-error"
import {
  startMicrophonePcmCapture,
  type MicrophonePcmCapture,
} from "@/lib/microphone-pcm-capture"
import {
  cancelRealtimeVoice,
  finishRealtimeVoice,
  pushRealtimeVoiceAudio,
  startRealtimeVoice,
  type RealtimeVoiceEvent,
} from "@/lib/realtime-voice"
import {
  useVoiceAutoSend,
  useVoiceCleanup,
  useVoiceLifecycle,
  useVoiceRuntime,
  useVoiceView,
  type RealtimeVoiceErrorKind,
  type VoiceRuntime,
  type VoiceViewActions,
} from "./realtime-voice-runtime"

export type {
  RealtimeVoiceErrorKind,
  RealtimeVoiceStatus,
} from "./realtime-voice-runtime"

interface UseRealtimeVoiceInputOptions {
  enabled: boolean
  scopeKey: string | null
  onFinal: (text: string) => void
  onAutoSend: () => void
  onError: (kind: RealtimeVoiceErrorKind) => void
}

interface VoiceStartOptions {
  enabled: boolean
  runtime: VoiceRuntime
  actions: VoiceViewActions
  handleEvent: (event: RealtimeVoiceEvent) => void
  enqueueAudio: (chunk: Uint8Array) => void
  fail: (kind: RealtimeVoiceErrorKind, cancelBackend: boolean) => Promise<void>
}

export function useRealtimeVoiceInput(options: UseRealtimeVoiceInputOptions) {
  const view = useVoiceView()
  const runtime = useVoiceRuntime(options.scopeKey, options)
  const setAutoSend = useVoiceAutoSend(runtime, view.actions)
  const cleanup = useVoiceCleanup(runtime, view.actions)
  const handleEvent = useVoiceEvents(runtime, view.actions, cleanup)
  const enqueueAudio = useVoiceAudio(runtime, cleanup.fail)
  const start = useVoiceStart({
    enabled: options.enabled,
    runtime,
    actions: view.actions,
    handleEvent,
    enqueueAudio,
    fail: cleanup.fail,
  })
  const stop = useVoiceStop(runtime, view.actions, cleanup.fail)
  const toggle = useCallback(() => {
    if (runtime.phase === "idle") void start()
    else if (runtime.phase === "recording") void stop()
  }, [runtime, start, stop])
  useVoiceLifecycle({
    runtime,
    cancel: cleanup.cancel,
    enabled: options.enabled,
    status: view.status,
    scopeKey: options.scopeKey,
  })
  return {
    status: view.status,
    partialText: view.partialText,
    autoSend: view.autoSend,
    setAutoSend,
    toggle,
  }
}

function useVoiceEvents(
  runtime: VoiceRuntime,
  actions: VoiceViewActions,
  cleanup: ReturnType<typeof useVoiceCleanup>
) {
  return useCallback(
    (event: RealtimeVoiceEvent) => {
      if (
        event.type === "ready" ||
        event.sessionId !== runtime.session?.sessionId
      )
        return
      if (event.type === "partial") {
        if (runtime.mounted) actions.setPartialText(event.text)
        return
      }
      if (event.type === "final") {
        handleFinalEvent(event, runtime, actions)
        return
      }
      if (event.type === "completed") {
        const shouldSend = runtime.userStopped && runtime.hadFinal
        void cleanup.reset().then(() => {
          if (shouldSend && runtime.autoSend && runtime.mounted) {
            runtime.onAutoSend()
          }
        })
        return
      }
      void cleanup.fail("serviceUnavailable", false)
    },
    [actions, cleanup, runtime]
  )
}

function handleFinalEvent(
  event: Extract<RealtimeVoiceEvent, { type: "final" }>,
  runtime: VoiceRuntime,
  actions: VoiceViewActions
): void {
  if (!runtime.acceptFinal(event.sequence)) return
  if (runtime.mounted) actions.setPartialText("")
  const text = event.text.trim()
  if (!text) return
  runtime.markFinal()
  if (runtime.mounted) runtime.onFinal(text)
}

function useVoiceAudio(runtime: VoiceRuntime, fail: VoiceStartOptions["fail"]) {
  return useCallback(
    (chunk: Uint8Array) => {
      const sessionId = runtime.session?.sessionId
      if (!sessionId || runtime.audioFailed) return
      runtime.enqueueAudio(async () => {
        if (!canPushAudio(runtime, sessionId)) return
        try {
          await pushRealtimeVoiceAudio(sessionId, chunk)
        } catch {
          if (runtime.session?.sessionId !== sessionId) return
          runtime.markAudioFailed()
          await fail("serviceUnavailable", true)
        }
      })
    },
    [fail, runtime]
  )
}

function canPushAudio(runtime: VoiceRuntime, sessionId: string): boolean {
  return !runtime.audioFailed && runtime.session?.sessionId === sessionId
}

function useVoiceStart(options: VoiceStartOptions) {
  return useCallback(async () => {
    const { runtime, actions } = options
    if (!options.enabled || runtime.phase !== "idle") return
    const operation = runtime.prepareStart()
    actions.setStatus("starting")
    try {
      const session = await startRealtimeVoice(options.handleEvent)
      if (!isCurrentOperation(runtime, operation)) {
        await cancelRealtimeVoice(session.sessionId).catch(() => {})
        return
      }
      runtime.activateSession(session)
      const capture = await startMicrophonePcmCapture(options.enqueueAudio)
      if (!isCurrentOperation(runtime, operation)) {
        await discardCapture(capture, session.sessionId)
        return
      }
      runtime.activateCapture(capture)
      actions.setStatus("recording")
    } catch (error) {
      if (!isCurrentOperation(runtime, operation)) return
      await options.fail(classifyStartError(error), true)
    }
  }, [options])
}

function useVoiceStop(
  runtime: VoiceRuntime,
  actions: VoiceViewActions,
  fail: VoiceStartOptions["fail"]
) {
  return useCallback(async () => {
    const sessionId = runtime.beginStop()
    if (!sessionId) return
    actions.setStatus("stopping")
    try {
      const capture = runtime.takeCapture()
      if (capture) await capture.stop()
      await runtime.audioQueue
      if (runtime.session?.sessionId !== sessionId) return
      await finishRealtimeVoice(sessionId)
    } catch {
      await fail("serviceUnavailable", true)
    }
  }, [actions, fail, runtime])
}

function isCurrentOperation(runtime: VoiceRuntime, operation: number): boolean {
  return runtime.isCurrent(operation)
}

async function discardCapture(
  capture: MicrophonePcmCapture,
  sessionId: string
): Promise<void> {
  await capture.stop().catch(() => {})
  await cancelRealtimeVoice(sessionId).catch(() => {})
}

function classifyStartError(error: unknown): RealtimeVoiceErrorKind {
  if (error instanceof DOMException) {
    if (error.name === "NotAllowedError" || error.name === "SecurityError") {
      return "microphoneDenied"
    }
    return "microphoneUnavailable"
  }
  return extractAppCommandError(error)?.code === "authentication_failed"
    ? "loginRequired"
    : "serviceUnavailable"
}
