import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react"

import {
  cancelRealtimeVoice,
  loadRealtimeVoiceAutoSend,
  saveRealtimeVoiceAutoSend,
  type RealtimeVoiceSession,
} from "@/lib/realtime-voice"
import type { MicrophonePcmCapture } from "@/lib/microphone-pcm-capture"

export type RealtimeVoiceStatus = "idle" | "starting" | "recording" | "stopping"

export type RealtimeVoiceErrorKind =
  | "loginRequired"
  | "microphoneDenied"
  | "microphoneUnavailable"
  | "serviceUnavailable"

export class VoiceRuntime {
  session: RealtimeVoiceSession | null
  capture: MicrophonePcmCapture | null
  audioQueue: Promise<void>
  audioFailed: boolean
  operation: number
  phase: RealtimeVoiceStatus
  finalSequence: number
  hadFinal: boolean
  userStopped: boolean
  mounted: boolean
  autoSend: boolean
  previousScopeKey: string | null
  onFinal: (text: string) => void
  onAutoSend: () => void
  onError: (kind: RealtimeVoiceErrorKind) => void

  constructor(scopeKey: string | null, callbacks: VoiceCallbacks) {
    this.session = null
    this.capture = null
    this.audioQueue = Promise.resolve()
    this.audioFailed = false
    this.operation = 0
    this.phase = "idle"
    this.finalSequence = -1
    this.hadFinal = false
    this.userStopped = false
    this.mounted = true
    this.autoSend = false
    this.previousScopeKey = scopeKey
    this.onFinal = callbacks.onFinal
    this.onAutoSend = callbacks.onAutoSend
    this.onError = callbacks.onError
  }

  updateCallbacks(callbacks: VoiceCallbacks): void {
    this.onFinal = callbacks.onFinal
    this.onAutoSend = callbacks.onAutoSend
    this.onError = callbacks.onError
  }

  mount(): void {
    this.mounted = true
  }

  setAutoSend(value: boolean): void {
    this.autoSend = value
  }

  reset(): MicrophonePcmCapture | null {
    this.operation += 1
    const capture = this.capture
    this.capture = null
    this.session = null
    this.phase = "idle"
    this.audioQueue = Promise.resolve()
    return capture
  }

  prepareStart(): number {
    this.operation += 1
    this.phase = "starting"
    this.audioFailed = false
    this.finalSequence = -1
    this.hadFinal = false
    this.userStopped = false
    return this.operation
  }

  isCurrent(operation: number): boolean {
    return this.mounted && this.operation === operation
  }

  activateSession(session: RealtimeVoiceSession): void {
    this.session = session
  }

  holdCapture(capture: MicrophonePcmCapture): void {
    this.capture = capture
  }

  activateCapture(): void {
    this.phase = "recording"
  }

  beginStop(): string | null {
    if (this.phase !== "recording") return null
    this.phase = "stopping"
    this.userStopped = true
    return this.session?.sessionId ?? null
  }

  takeCapture(): MicrophonePcmCapture | null {
    const capture = this.capture
    this.capture = null
    return capture
  }

  enqueueAudio(task: () => Promise<void>): void {
    this.audioQueue = this.audioQueue.then(task)
  }

  markAudioFailed(): void {
    this.audioFailed = true
  }

  acceptFinal(sequence: number): boolean {
    if (sequence <= this.finalSequence) return false
    this.finalSequence = sequence
    return true
  }

  markFinal(): void {
    this.hadFinal = true
  }

  changeScope(scopeKey: string | null): boolean {
    if (this.previousScopeKey === scopeKey) return false
    this.previousScopeKey = scopeKey
    return true
  }

  unmount(): { capture: MicrophonePcmCapture | null; sessionId?: string } {
    this.mounted = false
    this.operation += 1
    return {
      capture: this.capture,
      sessionId: this.session?.sessionId,
    }
  }
}

export interface VoiceViewActions {
  setStatus: Dispatch<SetStateAction<RealtimeVoiceStatus>>
  setPartialText: Dispatch<SetStateAction<string>>
  setAutoSend: Dispatch<SetStateAction<boolean>>
}

interface VoiceCallbacks {
  onFinal: VoiceRuntime["onFinal"]
  onAutoSend: VoiceRuntime["onAutoSend"]
  onError: VoiceRuntime["onError"]
}

interface VoiceLifecycleOptions {
  runtime: VoiceRuntime
  cancel: () => Promise<void>
  enabled: boolean
  status: RealtimeVoiceStatus
  scopeKey: string | null
}

export function useVoiceView() {
  const [status, setStatus] = useState<RealtimeVoiceStatus>("idle")
  const [partialText, setPartialText] = useState("")
  const [autoSend, setAutoSend] = useState(false)
  const actions = useMemo(
    () => ({ setStatus, setPartialText, setAutoSend }),
    []
  )
  return { status, partialText, autoSend, actions }
}

export function useVoiceRuntime(
  scopeKey: string | null,
  callbacks: VoiceCallbacks
): VoiceRuntime {
  const [runtime] = useState(() => new VoiceRuntime(scopeKey, callbacks))
  useEffect(() => {
    runtime.updateCallbacks(callbacks)
  }, [callbacks, runtime])
  return runtime
}

export function useVoiceAutoSend(
  runtime: VoiceRuntime,
  actions: VoiceViewActions
) {
  useEffect(() => {
    const frame = requestAnimationFrame(() => {
      const stored = loadRealtimeVoiceAutoSend()
      runtime.setAutoSend(stored)
      actions.setAutoSend(stored)
    })
    return () => cancelAnimationFrame(frame)
  }, [actions, runtime])
  return useCallback(
    (value: boolean) => {
      runtime.setAutoSend(value)
      actions.setAutoSend(value)
      saveRealtimeVoiceAutoSend(value)
    },
    [actions, runtime]
  )
}

export function useVoiceCleanup(
  runtime: VoiceRuntime,
  actions: VoiceViewActions
) {
  const reset = useCallback(async () => {
    const capture = runtime.reset()
    if (capture) await capture.stop().catch(() => {})
    if (!runtime.mounted) return
    actions.setPartialText("")
    actions.setStatus("idle")
  }, [actions, runtime])
  const fail = useCallback(
    async (kind: RealtimeVoiceErrorKind, cancelBackend: boolean) => {
      const sessionId = runtime.session?.sessionId
      await reset()
      if (cancelBackend && sessionId) {
        await cancelRealtimeVoice(sessionId).catch(() => {})
      }
      if (runtime.mounted) runtime.onError(kind)
    },
    [reset, runtime]
  )
  const cancel = useCallback(async () => {
    const sessionId = runtime.session?.sessionId
    await reset()
    if (sessionId) await cancelRealtimeVoice(sessionId).catch(() => {})
  }, [reset, runtime])
  return { reset, fail, cancel }
}

export function useVoiceLifecycle({
  runtime,
  cancel,
  enabled,
  status,
  scopeKey,
}: VoiceLifecycleOptions): void {
  useEffect(() => {
    if (!enabled && status !== "idle") queueMicrotask(() => void cancel())
  }, [cancel, enabled, status])
  useEffect(() => {
    if (!runtime.changeScope(scopeKey)) return
    queueMicrotask(() => void cancel())
  }, [cancel, runtime, scopeKey])
  useEffect(() => {
    runtime.mount()
    return () => {
      const { capture, sessionId } = runtime.unmount()
      void capture?.stop()
      if (sessionId) void cancelRealtimeVoice(sessionId).catch(() => {})
    }
  }, [runtime])
}
