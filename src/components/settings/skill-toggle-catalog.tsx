import { Badge } from "@/components/ui/badge"
import { Switch } from "@/components/ui/switch"
import { cn } from "@/lib/utils"
import type { ManagedSkillState } from "@/lib/types"
import type { SkillToggleItem } from "./skill-toggle-list-model"

interface SkillToggleCatalogProps {
  groups: Array<[string, SkillToggleItem[]]>
  skillStates: ReadonlyMap<string, ManagedSkillState>
  pendingSkillIds: ReadonlySet<string>
  emptyText: string
  previewEnabled: boolean
  notReadyHint?: string
  translateCategory: (category: string) => string
  onOpenDetail: (skillId: string) => void
  onToggleSkill: (skillId: string, enabled: boolean) => void
}

export function SkillToggleCatalog({
  groups,
  skillStates,
  pendingSkillIds,
  emptyText,
  previewEnabled,
  notReadyHint,
  translateCategory,
  onOpenDetail,
  onToggleSkill,
}: SkillToggleCatalogProps) {
  if (groups.length === 0) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-sm text-muted-foreground">
        {emptyText}
      </div>
    )
  }

  return groups.map(([category, items]) => (
    <section key={category}>
      <h3 className="border-b bg-muted/35 px-3 py-2 text-xs font-semibold text-muted-foreground">
        {translateCategory(category)}
      </h3>
      {items.map((skill) => {
        const state = skillStates.get(skill.id)
        const ready = state?.ready ?? skill.ready
        const enabled = state?.enabled ?? false
        const pending = pendingSkillIds.has(skill.id)
        return (
          <div
            key={skill.id}
            className="flex min-h-14 items-center gap-3 border-b px-3 py-2 last:border-b-0"
            title={!ready ? notReadyHint : undefined}
          >
            {previewEnabled ? (
              <button
                type="button"
                onClick={() => onOpenDetail(skill.id)}
                className="min-w-0 flex-1 text-left"
              >
                <span className="block truncate text-sm font-medium">
                  {skill.displayName}
                </span>
                <span className="block truncate text-xs text-muted-foreground">
                  {skill.description}
                </span>
              </button>
            ) : (
              <div className="min-w-0 flex-1">
                <div className="truncate text-sm font-medium">
                  {skill.displayName}
                </div>
                <div className="truncate text-xs text-muted-foreground">
                  {skill.description}
                </div>
              </div>
            )}
            {skill.badge ? (
              <Badge
                variant="outline"
                className={cn(
                  "shrink-0 text-[10px]",
                  skill.badge.tone === "amber"
                    ? "border-amber-500/40 bg-amber-500/10 text-amber-600 dark:text-amber-400"
                    : "text-muted-foreground"
                )}
              >
                {skill.badge.label}
              </Badge>
            ) : null}
            <Switch
              checked={enabled}
              onCheckedChange={(enabled) => onToggleSkill(skill.id, enabled)}
              disabled={pending || (!ready && !enabled)}
              aria-label={skill.displayName}
              className="shrink-0"
            />
          </div>
        )
      })}
    </section>
  ))
}
