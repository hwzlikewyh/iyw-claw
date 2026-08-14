"use client"

import type { ComponentType } from "react"

import {
  CloudBoatIcon,
  FarMountainIcon,
  FlowingLightIcon,
  GreenMistBuddyIcon,
  HermesGlyph,
  InkRiverPiIcon,
  MoonWhiteIcon,
  OpenClawGlyph,
  StarRiverIcon,
  TinyFocusIcon,
  WindChaserIcon,
  type AgentGlyphProps,
} from "@/components/agent-icons/cartoon-agent-icons"
import type { AgentType } from "@/lib/types"
import { cn } from "@/lib/utils"

interface AgentIconProps {
  agentType: AgentType
  className?: string
}

const AGENT_ICONS: Record<AgentType, ComponentType<AgentGlyphProps>> = {
  claude_code: FarMountainIcon,
  codex: StarRiverIcon,
  gemini: FlowingLightIcon,
  open_claw: OpenClawGlyph,
  open_code: CloudBoatIcon,
  cline: WindChaserIcon,
  hermes: HermesGlyph,
  code_buddy: GreenMistBuddyIcon,
  kimi_code: MoonWhiteIcon,
  pi: InkRiverPiIcon,
  grok: TinyFocusIcon,
}

export function AgentIcon({ agentType, className }: AgentIconProps) {
  const Icon = AGENT_ICONS[agentType]
  return (
    <span className={cn("inline-flex shrink-0", className)}>
      <Icon size="100%" />
    </span>
  )
}
