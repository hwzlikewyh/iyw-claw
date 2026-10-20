import { getTransport } from "@/lib/transport"
import type { Scenario } from "@/lib/types"

export interface ScenarioCase {
  model?: string | null
  id: string
  categoryKey: string
  scenarioId: string
  title: string
  summary: string
  tag: string
  coverKey: "stationery" | "survey" | "ceramics" | "quotation"
  coverTone: "blue" | "peach" | "purple" | "cyan" | "yellow"
  tagAtFooter: boolean
  sortOrder: number
}

export interface ScenarioCaseAsset {
  label?: string | null
  step?: string | null
  fileName: string
  kind: "image" | "pdf" | "video" | "audio" | "markdown" | "text" | "file"
  url: string
  expiresAt: string
}

export interface ScenarioCaseDetail extends ScenarioCase {
  promptTemplate?: string
  scenario: Scenario
  assets: ScenarioCaseAsset[]
}

export function scenarioCasesCatalog() {
  return getTransport().call<{ items: ScenarioCase[] }>(
    "scenario_cases_catalog"
  )
}

export function scenarioCaseDetail(id: string) {
  return getTransport().call<ScenarioCaseDetail>("scenario_case_detail", { id })
}
