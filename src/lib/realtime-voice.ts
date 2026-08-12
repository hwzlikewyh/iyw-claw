import { Channel, invoke } from "@tauri-apps/api/core"

const AUTO_SEND_STORAGE_KEY = "iyw-claw-realtime-voice-auto-send"
const MAX_AUDIO_CHUNK_BYTES = 12_800

export type RealtimeVoiceEvent =
  | { type: "ready"; sessionId: string }
  | {
      type: "partial" | "final"
      sessionId: string
      sequence: number
      text: string
      startMs?: number | null
      endMs?: number | null
    }
  | { type: "completed"; sessionId: string; durationMs?: number | null }
  | { type: "error"; sessionId: string; code: string; message: string }

export interface RealtimeVoiceSession {
  sessionId: string
  channel: Channel<RealtimeVoiceEvent>
}

export async function startRealtimeVoice(
  onEvent: (event: RealtimeVoiceEvent) => void
): Promise<RealtimeVoiceSession> {
  const channel = new Channel<RealtimeVoiceEvent>()
  channel.onmessage = onEvent
  const result = await invoke<{ sessionId: string }>("realtime_voice_start", {
    onEvent: channel,
  })
  return { sessionId: result.sessionId, channel }
}

export async function pushRealtimeVoiceAudio(
  sessionId: string,
  chunk: Uint8Array
): Promise<void> {
  if (
    chunk.length === 0 ||
    chunk.length > MAX_AUDIO_CHUNK_BYTES ||
    chunk.length % 2 !== 0
  ) {
    throw new Error("Invalid realtime voice audio chunk")
  }
  await invoke("realtime_voice_push_audio", {
    sessionId,
    chunk: Array.from(chunk),
  })
}

export async function finishRealtimeVoice(sessionId: string): Promise<void> {
  await invoke("realtime_voice_finish", { sessionId })
}

export async function cancelRealtimeVoice(sessionId: string): Promise<void> {
  await invoke("realtime_voice_cancel", { sessionId })
}

export function loadRealtimeVoiceAutoSend(): boolean {
  try {
    return globalThis.localStorage?.getItem(AUTO_SEND_STORAGE_KEY) === "true"
  } catch {
    return false
  }
}

export function saveRealtimeVoiceAutoSend(value: boolean): void {
  try {
    globalThis.localStorage?.setItem(AUTO_SEND_STORAGE_KEY, String(value))
  } catch {
    // Storage can be unavailable in hardened WebViews; the setting stays local.
  }
}
