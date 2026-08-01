"use client"

import { Suspense } from "react"
import { SkillMarketView } from "@/components/skills/market/view"

export function SkillMarketPage() {
  return (
    <Suspense fallback={null}>
      <SkillMarketView />
    </Suspense>
  )
}
