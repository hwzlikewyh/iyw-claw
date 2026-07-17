"use client"

import {
  ClipboardCheck,
  FlaskConical,
  Gauge,
  GraduationCap,
  LineChart,
  Quote,
  ScanSearch,
  ScatterChart,
  Search,
  Sigma,
  Sparkles,
  TestTubes,
  Workflow,
  type LucideIcon,
} from "lucide-react"

export const SCIENCE_ICON_MAP: Record<string, LucideIcon> = {
  Sparkles,
  FlaskConical,
  TestTubes,
  Gauge,
  Sigma,
  ScatterChart,
  LineChart,
  ScanSearch,
  Search,
  ClipboardCheck,
  Quote,
  GraduationCap,
  Workflow,
}

export function getScienceIcon(name: string | null | undefined): LucideIcon {
  return (name && SCIENCE_ICON_MAP[name]) || FlaskConical
}
