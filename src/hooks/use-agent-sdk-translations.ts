"use client"

import { useMemo } from "react"
import { useTranslations } from "next-intl"

import { maskAgentSdkTranslator } from "@/lib/agent-sdk-presentation"

export function useAgentSdkTranslations() {
  const translate = useTranslations("AcpAgentSettings")
  return useMemo(() => maskAgentSdkTranslator(translate), [translate])
}
