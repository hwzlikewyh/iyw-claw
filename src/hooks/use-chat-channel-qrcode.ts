"use client"

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react"

import {
  cancelChatChannelQr,
  pollChatChannelQr,
  startChatChannelQr,
  type ChatChannelQrPoll,
  type ChatChannelQrStart,
  type ChatChannelQrStatus,
} from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import type { ChannelType } from "@/lib/types"

export type ChatChannelQrViewStatus = "loading" | ChatChannelQrStatus

export interface ChatChannelQrViewState {
  status: ChatChannelQrViewStatus
  session: ChatChannelQrStart | null
  error: string | null
  errorCode: string | null
}

const INITIAL_STATE: ChatChannelQrViewState = {
  status: "loading",
  session: null,
  error: null,
  errorCode: null,
}

const TERMINAL = new Set<ChatChannelQrStatus>([
  "connected",
  "expired",
  "denied",
  "cancelled",
  "error",
])
const MAX_CONSECUTIVE_POLL_FAILURES = 5
const DEFAULT_RETRY_MS = 3000

interface QrRunOptions {
  channelId: number
  channelType: ChannelType
  variant?: "feishu" | "lark"
  onConnected: () => void
  setState: Dispatch<SetStateAction<ChatChannelQrViewState>>
}

class QrPollingRun {
  private disposed = false
  private connected = false
  private polling = false
  private pollFailures = 0
  private retryAfterMs = DEFAULT_RETRY_MS
  private sessionId: string | null = null
  private timer: ReturnType<typeof setTimeout> | null = null

  constructor(private readonly options: QrRunOptions) {}

  async start() {
    try {
      const session = await startChatChannelQr(this.options)
      if (this.disposed) {
        void cancelChatChannelQr(session.sessionId).catch(() => {})
        return
      }
      this.sessionId = session.sessionId
      this.options.setState({
        status: session.status,
        session,
        error: null,
        errorCode: null,
      })
      this.retryAfterMs = session.retryAfterMs
      this.schedule(session.retryAfterMs)
    } catch (error) {
      if (!this.disposed) this.fail(error, "start_failed")
    }
  }

  async poll(verifyCode?: string) {
    if (!this.sessionId || this.disposed || this.polling) return
    this.polling = true
    this.clearTimer()
    try {
      const result = await pollChatChannelQr(this.sessionId, verifyCode)
      this.pollFailures = 0
      this.retryAfterMs = result.retryAfterMs
      if (!this.apply(result)) return
      if (
        !TERMINAL.has(result.status) &&
        result.errorCode !== "verify_code_required"
      ) {
        this.schedule(result.retryAfterMs)
      }
    } catch (error) {
      if (!this.disposed) {
        this.pollFailures += 1
        if (this.pollFailures < MAX_CONSECUTIVE_POLL_FAILURES) {
          this.schedule(this.retryAfterMs)
        } else {
          this.fail(error, "poll_failed")
        }
      }
    } finally {
      this.polling = false
    }
  }

  dispose() {
    this.disposed = true
    this.clearTimer()
    if (this.sessionId && !this.connected) {
      void cancelChatChannelQr(this.sessionId).catch(() => {})
    }
    this.sessionId = null
  }

  private apply(result: ChatChannelQrPoll) {
    if (this.disposed || result.sessionId !== this.sessionId) return false
    this.options.setState((current) => ({
      ...current,
      status: result.status,
      error: null,
      errorCode: result.errorCode ?? null,
    }))
    if (result.status === "connected" && !this.connected) {
      this.connected = true
      this.options.onConnected()
    }
    return true
  }

  private schedule(delayMs: number) {
    this.clearTimer()
    this.timer = setTimeout(() => void this.poll(), delayMs)
  }

  private clearTimer() {
    if (this.timer) clearTimeout(this.timer)
    this.timer = null
  }

  private fail(error: unknown, errorCode: string) {
    this.options.setState((current) => ({
      ...current,
      status: "error",
      error: toErrorMessage(error),
      errorCode,
    }))
  }
}

export function useChatChannelQrcode({
  active,
  channelId,
  channelType,
  variant,
  onConnected,
}: Omit<QrRunOptions, "setState"> & { active: boolean }) {
  const [state, setState] = useState(INITIAL_STATE)
  const [attempt, setAttempt] = useState(0)
  const runRef = useRef<QrPollingRun | null>(null)
  const onConnectedRef = useRef(onConnected)

  useEffect(() => {
    onConnectedRef.current = onConnected
  }, [onConnected])

  useEffect(() => {
    if (!active) return
    // eslint-disable-next-line react-hooks/set-state-in-effect -- reset for a fresh provider session
    setState(INITIAL_STATE)
    const run = new QrPollingRun({
      channelId,
      channelType,
      variant,
      onConnected: () => onConnectedRef.current(),
      setState,
    })
    runRef.current = run
    void run.start()
    return () => {
      run.dispose()
      if (runRef.current === run) runRef.current = null
    }
  }, [active, attempt, channelId, channelType, variant])

  const restart = useCallback(() => setAttempt((value) => value + 1), [])
  const submitVerifyCode = useCallback(
    (code: string) => runRef.current?.poll(code),
    []
  )
  return { state, restart, submitVerifyCode }
}
