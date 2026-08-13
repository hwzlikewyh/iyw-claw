"use client"

import { SlidersHorizontal } from "lucide-react"
import { useTranslations } from "next-intl"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import type { SkillMarketQueryState } from "@/hooks/use-skill-market"
import type {
  SkillMarketCategory,
  SkillMarketTranslator,
} from "@/lib/skill-market"
import { cn } from "@/lib/utils"

interface FilterPopoverProps {
  query: SkillMarketQueryState
  categories: SkillMarketCategory[]
  onQueryChange: (patch: Partial<SkillMarketQueryState>) => void
}

export function FilterPopover({
  query,
  categories,
  onQueryChange,
}: FilterPopoverProps) {
  const t = useTranslations("SkillMarketV2") as unknown as SkillMarketTranslator
  const activeCount =
    (query.publisher !== "all" ? 1 : 0) +
    (query.distribution !== "all" ? 1 : 0) +
    (query.compatibility !== "all" ? 1 : 0) +
    (query.category ? 1 : 0)
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          size="sm"
          variant="outline"
          className={cn(
            "h-8",
            activeCount > 0 && "border-primary/40 text-primary"
          )}
          aria-label={t("filters.label")}
        >
          <SlidersHorizontal className="size-3.5" aria-hidden="true" />
          {t("filters.label")}
          {activeCount > 0 ? (
            <Badge variant="secondary" className="ml-0.5 h-4 min-w-4 px-1">
              {activeCount}
            </Badge>
          ) : null}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-72 gap-3">
        <FilterSelect
          label={t("filters.publisher")}
          value={query.publisher}
          onChange={(publisher) =>
            onQueryChange({
              publisher: publisher as SkillMarketQueryState["publisher"],
            })
          }
          options={[
            ["all", t("filters.all")],
            ["official", t("filters.official")],
            ["user", t("filters.user")],
          ]}
        />
        <FilterSelect
          label={t("filters.distribution")}
          value={query.distribution}
          onChange={(distribution) =>
            onQueryChange({
              distribution:
                distribution as SkillMarketQueryState["distribution"],
            })
          }
          options={[
            ["all", t("filters.all")],
            ["mandatory", t("filters.mandatory")],
            ["optional", t("filters.optional")],
          ]}
        />
        <FilterSelect
          label={t("filters.compatibility")}
          value={query.compatibility}
          onChange={(compatibility) =>
            onQueryChange({
              compatibility:
                compatibility as SkillMarketQueryState["compatibility"],
            })
          }
          options={[
            ["all", t("filters.all")],
            ["compatible", t("filters.compatible")],
            ["incompatible", t("filters.incompatible")],
            ["unknown", t("filters.unknown")],
          ]}
        />
        <FilterSelect
          label={t("filters.category")}
          value={query.category ?? "all"}
          onChange={(category) =>
            onQueryChange({ category: category === "all" ? null : category })
          }
          options={[
            ["all", t("filters.categoryAll")],
            ...categories.map(
              (category) =>
                [category.key, t(`categories.${category.key}`)] as const
            ),
          ]}
        />
        <Button
          size="sm"
          variant="outline"
          className="w-full"
          disabled={activeCount === 0}
          onClick={() =>
            onQueryChange({
              publisher: "all",
              distribution: "all",
              compatibility: "all",
              category: null,
            })
          }
        >
          {t("filters.reset")}
        </Button>
      </PopoverContent>
    </Popover>
  )
}

function FilterSelect({
  label,
  value,
  options,
  onChange,
}: {
  label: string
  value: string
  options: ReadonlyArray<readonly [string, string]>
  onChange: (value: string) => void
}) {
  return (
    <div className="grid gap-2">
      <label className="text-xs font-medium text-muted-foreground">
        {label}
      </label>
      <Select value={value} onValueChange={onChange}>
        <SelectTrigger className="w-full rounded-md">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {options.map(([optionValue, optionLabel]) => (
            <SelectItem key={optionValue} value={optionValue}>
              {optionLabel}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}
