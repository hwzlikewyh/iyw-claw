import { reportSendEvent } from "@/lib/send-diagnostics"

const DEADLINE_MS = {
  connect_timeout: 150_000,
  mode_timeout: 30_000,
  context_timeout: 150_000,
} as const

export class SendPreflightTimeout extends Error {}

export function waitForSendPreflight<T>(
  operation: Promise<T>,
  stage: keyof typeof DEADLINE_MS
): Promise<T> {
  let timer: ReturnType<typeof setTimeout>
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => {
      reportSendEvent(stage)
      reject(new SendPreflightTimeout(`${stage}: request timed out`))
    }, DEADLINE_MS[stage])
  })
  return Promise.race([operation, timeout]).finally(() => clearTimeout(timer))
}
