"use client"

import { useEffect } from "react"
import { useTranslations } from "next-intl"

import { getWebServerStatus, listChatChannels } from "@/lib/api"
import { parseChannelConfig } from "@/lib/chat-channel-setup"
import type { ChatChannelInfo } from "@/lib/types"
import type { WecomAgentStep } from "./wecom-agent-setup-parts"

export function useWebServerProbe(
  open: boolean,
  setRunning: (running: boolean | null) => void
) {
  useEffect(() => {
    if (!open) return
    getWebServerStatus()
      .then((status) => setRunning(status !== null))
      .catch(() => setRunning(null))
  }, [open, setRunning])
}

export function useCallbackVerificationPoll({
  open,
  step,
  working,
  verified,
  setVerified,
  setError,
}: {
  open: boolean
  step: WecomAgentStep
  working?: ChatChannelInfo
  verified: boolean
  setVerified: (verified: boolean) => void
  setError: (error: string | null) => void
}) {
  const t = useTranslations("ChatChannelSettings")
  useEffect(() => {
    if (!open || step !== 3 || !working || verified) return
    const poll = async () => {
      try {
        const current = (await listChatChannels()).find(
          (channel) => channel.id === working.id
        )
        if (current && parseChannelConfig(current).callback_verified_at) {
          setVerified(true)
          setError(null)
        }
      } catch {
        setError(t("market.wecomAgent.callbackPollFailed"))
      }
    }
    void poll()
    const timer = window.setInterval(() => void poll(), 2000)
    return () => window.clearInterval(timer)
  }, [open, setError, setVerified, step, t, verified, working])
}
