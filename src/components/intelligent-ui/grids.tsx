"use client"

import { useTranslations } from "next-intl"
import { MarkdownLink } from "@/components/ai-elements/markdown-link"
import { list, record, text, toneClass, useViewState } from "./context"

export function StatGrid({ props }: { props: Record<string, unknown> }) {
  return (
    <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
      {list(props.items)
        .slice(0, 100)
        .map((item, index) => {
          const p = record(item)
          return (
            <div key={index}>
              <p className="text-xs text-muted-foreground">{text(p.label)}</p>
              <p className="text-xl font-semibold">
                {text(p.value)} <small>{text(p.unit ?? "")}</small>
              </p>
              <p
                className={`text-xs ${toneClass(p.trend === "up" ? "positive" : p.trend === "down" ? "negative" : "muted")}`}
              >
                {text(p.delta ?? "")}
              </p>
            </div>
          )
        })}
    </div>
  )
}
export function CardGrid({ props }: { props: Record<string, unknown> }) {
  const [filter, setFilter] = useViewState<string>("filter", "")
  const items = list(props.items).slice(0, 100).map(record)
  const tags = [
    ...new Set(
      items
        .map((item) => (typeof item.tag === "string" ? item.tag : ""))
        .filter(Boolean)
    ),
  ]
  return (
    <div className="space-y-3">
      <TagFilter tags={tags} filter={filter} change={setFilter} />
      <div className="grid gap-3 sm:grid-cols-2">
        {items
          .filter((item) => !filter || item.tag === filter)
          .map((item, i) => (
            <CardItem key={i} item={item} />
          ))}
      </div>
    </div>
  )
}
function CardItem({ item: p }: { item: Record<string, unknown> }) {
  return (
    <article className="rounded-lg border p-3">
      <h4 className="font-medium">
        {typeof p.url === "string" ? (
          <MarkdownLink href={p.url}>{text(p.title)}</MarkdownLink>
        ) : (
          text(p.title)
        )}
      </h4>
      {Boolean(p.summary) && (
        <p className="mt-1 text-xs text-muted-foreground">{text(p.summary)}</p>
      )}
      {Boolean(p.source) && <p className="mt-2 text-xs">{text(p.source)}</p>}
    </article>
  )
}

function TagFilter({
  tags,
  filter,
  change,
}: {
  tags: string[]
  filter: string
  change: (value: string) => void
}) {
  const t = useTranslations("IntelligentUI")
  if (!tags.length) return null
  return (
    <div className="flex flex-wrap gap-1">
      <button
        aria-label={t("reset")}
        onClick={() => change("")}
        className="rounded border px-2 py-1"
      >
        ×
      </button>
      {tags.map((tag) => (
        <button
          key={tag}
          aria-pressed={filter === tag}
          onClick={() => change(filter === tag ? "" : tag)}
          className="rounded border px-2 py-1 text-xs"
        >
          {tag}
        </button>
      ))}
    </div>
  )
}
