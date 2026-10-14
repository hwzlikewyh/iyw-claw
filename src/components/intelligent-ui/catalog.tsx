"use client"

import { useTranslations } from "next-intl"
import { hasOwn } from "@/lib/intelligent-ui/value"
import { text } from "./context"
import { Chart } from "./chart"
import { Quiz, DecisionTree } from "./specialized"
import { Gantt } from "./gantt"
import { Diff } from "./ui-diff"
import { FunctionPlot } from "./function-plot"
import { CardGrid, StatGrid } from "./grids"
import { DataTable } from "./data-table"

const CATALOG = {
  chart: Chart,
  "stat-grid": StatGrid,
  "card-grid": CardGrid,
  "data-table": DataTable,
  quiz: Quiz,
  "decision-tree": DecisionTree,
  gantt: Gantt,
  diff: Diff,
  "function-plot": FunctionPlot,
}
export function CatalogComponent({
  name,
  props,
}: {
  name: string
  props: Record<string, unknown>
}) {
  const t = useTranslations("IntelligentUI")
  const Component = hasOwn(CATALOG, name)
    ? CATALOG[name as keyof typeof CATALOG]
    : undefined
  return (
    <section className="min-w-0">
      {Boolean(props.title) && (
        <h3 className="mb-3 font-semibold">{text(props.title)}</h3>
      )}
      {Component ? (
        <Component props={props} />
      ) : (
        <p role="status" className="text-xs text-muted-foreground">
          {t("unknownComponent", { name })}
        </p>
      )}
    </section>
  )
}
