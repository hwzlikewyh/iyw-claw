import { getTransport } from "@/lib/transport"

type SendEvent =
  | "offline"
  | "account_blocked"
  | "composer_timeout"
  | "connect_timeout"
  | "mode_timeout"
  | "context_timeout"
  | "config_wait"
  | "queue_stalled"

const lastReported = new Map<SendEvent, number>()
const REPORT_INTERVAL_MS = 2_000

export function reportSendEvent(event: SendEvent, conversationId?: number) {
  const now = Date.now()
  if (now - (lastReported.get(event) ?? 0) < REPORT_INTERVAL_MS) return
  lastReported.set(event, now)
  void Promise.resolve()
    .then(() =>
      getTransport().call("report_frontend_event", { event, conversationId })
    )
    .catch(() => {})
}
