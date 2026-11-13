"use client"

import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { CapabilityCard, CAPABILITY_GRID_CLASS } from "./capability-card"
import { SkillCard } from "./skill-card"
import type {
  CatalogEntry,
  useCapabilityCatalog,
} from "./use-capability-catalog"

type ConnectorEntry = Extract<CatalogEntry, { kind: "connector" }>
type SkillEntry = Extract<CatalogEntry, { kind: "skill" | "plugin" }>

export interface CapabilityCatalogResultsProps {
  catalog: ReturnType<typeof useCapabilityCatalog>
  items: CatalogEntry[]
  page: number
  pageSize: number
  onConnectorSelect: (entry: ConnectorEntry) => void
  onSkillSelect: (item: SkillEntry["item"] | null) => void
}

export function CapabilityCatalogResults({
  catalog,
  items,
  page,
  pageSize,
  onConnectorSelect,
  onSkillSelect,
}: CapabilityCatalogResultsProps) {
  const market = useTranslations("SkillMarketV2")
  const visible = items.slice((page - 1) * pageSize, page * pageSize)
  return (
    <>
      {catalog.error ? (
        <div role="alert" className="py-8">
          <p>{catalog.error}</p>
          <Button variant="outline" onClick={catalog.refresh}>
            {market("list.retry")}
          </Button>
        </div>
      ) : (
        <div data-market-grid className={CAPABILITY_GRID_CLASS}>
          {catalog.loading
            ? Array.from({ length: pageSize }, (_, index) => (
                <Skeleton
                  data-market-skeleton
                  key={index}
                  className="h-44 rounded-[10px]"
                />
              ))
            : visible.map((entry) => (
                <CatalogCard
                  key={entry.key}
                  entry={entry}
                  onConnector={onConnectorSelect}
                  onSkill={onSkillSelect}
                />
              ))}
        </div>
      )}
    </>
  )
}

function CatalogCard({
  entry,
  onConnector,
  onSkill,
}: {
  entry: CatalogEntry
  onConnector: (entry: ConnectorEntry) => void
  onSkill: (item: SkillEntry["item"] | null) => void
}) {
  if (entry.kind !== "connector")
    return (
      <SkillCard
        item={entry.item}
        selected={false}
        onSelect={onSkill}
        onPrimaryAction={onSkill}
      />
    )
  return (
    <CapabilityCard
      kind="connector"
      name={entry.name}
      publisher={entry.item.owner || entry.providerName}
      summary={entry.item.description}
      category={entry.item.protocols.join(" / ")}
      source={entry.providerName}
      version={entry.item.latest_version}
      icon={entry.item.icon_url}
      installed={entry.installed}
      onOpen={() => onConnector(entry)}
    />
  )
}
