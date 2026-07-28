"use client"

import {
  PackageCheck,
  Plus,
  Search,
  Store,
  Upload,
  UserRound,
  WandSparkles,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import type {
  SkillMarketCategory,
  SkillMarketPublisher,
  SkillMarketVisibility,
} from "@/lib/skill-market"
import { cn } from "@/lib/utils"

export type SkillMarketSection = "market" | "installed" | "mine"

type Translator = (
  key: string,
  values?: Record<string, string | number>
) => string

type MarketTool = "upload" | "import" | "generate"

function HeaderActions({ onTool }: { onTool: (tool: MarketTool) => void }) {
  const t = useTranslations("SkillsSettings.market")
  return (
    <div className="flex flex-wrap gap-2">
      <Button size="sm" onClick={() => onTool("upload")}>
        <Upload className="size-3.5" />
        {t("actions.upload")}
      </Button>
      <Button size="sm" variant="outline" onClick={() => onTool("import")}>
        <Plus className="size-3.5" />
        {t("actions.import")}
      </Button>
      <Button size="sm" variant="outline" onClick={() => onTool("generate")}>
        <WandSparkles className="size-3.5" />
        {t("actions.generate")}
      </Button>
    </div>
  )
}

function HeaderTabs({
  section,
  onSectionChange,
}: {
  section: SkillMarketSection
  onSectionChange: (section: SkillMarketSection) => void
}) {
  const t = useTranslations("SkillsSettings.market")
  return (
    <Tabs
      value={section}
      onValueChange={(value) => onSectionChange(value as SkillMarketSection)}
      className="px-3 pb-2"
    >
      <TabsList
        variant="line"
        className="max-w-full justify-start overflow-x-auto"
      >
        <TabsTrigger value="market">
          <Store className="size-3.5" />
          {t("tabs.market")}
        </TabsTrigger>
        <TabsTrigger value="installed">
          <PackageCheck className="size-3.5" />
          {t("tabs.installed")}
        </TabsTrigger>
        <TabsTrigger value="mine">
          <UserRound className="size-3.5" />
          {t("tabs.mine")}
        </TabsTrigger>
      </TabsList>
    </Tabs>
  )
}

export function SkillMarketHeader({
  section,
  onSectionChange,
  onTool,
}: {
  section: SkillMarketSection
  onSectionChange: (section: SkillMarketSection) => void
  onTool: (tool: MarketTool) => void
}) {
  const t = useTranslations("SkillsSettings.market")
  return (
    <header className="shrink-0 border-b">
      <div className="flex flex-col gap-3 px-4 py-3 lg:flex-row lg:items-center lg:justify-between">
        <div className="min-w-0">
          <h1 className="text-base font-semibold">{t("title")}</h1>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {t("description")}
          </p>
        </div>
        <HeaderActions onTool={onTool} />
      </div>
      <HeaderTabs section={section} onSectionChange={onSectionChange} />
    </header>
  )
}

type FilterProps = {
  section: Exclude<SkillMarketSection, "installed">
  categories: SkillMarketCategory[]
  query: string
  category: string | null
  publisher: SkillMarketPublisher | "all"
  visibility: SkillMarketVisibility | "all"
  onQueryChange: (value: string) => void
  onCategoryChange: (value: string | null) => void
  onPublisherChange: (value: SkillMarketPublisher | "all") => void
  onVisibilityChange: (value: SkillMarketVisibility | "all") => void
}

function FilterSelectors(props: FilterProps) {
  const t = useTranslations("SkillsSettings.market")
  return (
    <div className="flex flex-col gap-2 sm:flex-row">
      <div className="relative min-w-0 flex-1">
        <Search className="pointer-events-none absolute left-3 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
        <Input
          className="pl-8"
          value={props.query}
          placeholder={t("filters.searchPlaceholder")}
          onChange={(event) => props.onQueryChange(event.target.value)}
        />
      </div>
      <Select
        value={props.publisher}
        onValueChange={(value) =>
          props.onPublisherChange(value as SkillMarketPublisher | "all")
        }
      >
        <SelectTrigger className="w-full rounded-md sm:w-40">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="all">{t("filters.allPublishers")}</SelectItem>
          <SelectItem value="official">{t("publisher.official")}</SelectItem>
          <SelectItem value="user">{t("publisher.user")}</SelectItem>
        </SelectContent>
      </Select>
      {props.section === "mine" ? <VisibilitySelect {...props} /> : null}
    </div>
  )
}

function VisibilitySelect(props: FilterProps) {
  const t = useTranslations("SkillsSettings.market")
  return (
    <Select
      value={props.visibility}
      onValueChange={(value) =>
        props.onVisibilityChange(value as SkillMarketVisibility | "all")
      }
    >
      <SelectTrigger className="w-full rounded-md sm:w-36">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="all">{t("filters.allVisibility")}</SelectItem>
        <SelectItem value="public">{t("visibility.public")}</SelectItem>
        <SelectItem value="private">{t("visibility.private")}</SelectItem>
      </SelectContent>
    </Select>
  )
}

function CategoryFilters(props: FilterProps) {
  const t = useTranslations("SkillsSettings.market") as unknown as Translator
  const buttonClass = (selected: boolean) =>
    cn(
      "shrink-0 rounded-md border px-2.5 py-1.5 text-xs",
      selected
        ? "border-primary/50 bg-primary/10 text-primary"
        : "text-muted-foreground hover:text-foreground"
    )
  return (
    <div className="flex max-w-full gap-1.5 overflow-x-auto pb-1">
      <button
        type="button"
        className={buttonClass(props.category == null)}
        onClick={() => props.onCategoryChange(null)}
      >
        {t("filters.allCategories")}
      </button>
      {props.categories.map((item) => (
        <button
          key={item.key}
          type="button"
          className={buttonClass(props.category === item.key)}
          onClick={() => props.onCategoryChange(item.key)}
        >
          {t(`categories.${item.key}`)}
        </button>
      ))}
    </div>
  )
}

export function SkillMarketFilters({
  section,
  categories,
  query,
  category,
  publisher,
  visibility,
  onQueryChange,
  onCategoryChange,
  onPublisherChange,
  onVisibilityChange,
}: FilterProps) {
  const props = {
    section,
    categories,
    query,
    category,
    publisher,
    visibility,
    onQueryChange,
    onCategoryChange,
    onPublisherChange,
    onVisibilityChange,
  }
  return (
    <div className="space-y-3 border-b pb-4">
      <FilterSelectors {...props} />
      <CategoryFilters {...props} />
    </div>
  )
}
