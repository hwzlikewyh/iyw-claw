"use client"

import { Building2, Check, UserRound } from "lucide-react"
import { useTranslations } from "next-intl"
import type { IywWorkspace } from "@/lib/iyw-account-flow"
import { cn } from "@/lib/utils"

export function WorkspaceRows({
  spaces,
  selected,
  current,
  onSelect,
  disabled = false,
}: {
  spaces: IywWorkspace[]
  selected?: string
  current?: string | null
  onSelect: (id: string) => void
  disabled?: boolean
}) {
  const t = useTranslations("AccountFlow")
  return (
    <div className="grid gap-1.5" role="group" aria-label={t("chooseSpace")}>
      {([false, true] as const).map((company) => {
        const list = spaces.filter(
          (space) => (space.org_flag === 2) === company
        )
        if (!list.length) return null
        return (
          <div key={String(company)} className="grid gap-1">
            <div className="px-2 pb-1 pt-2 text-[10px] font-medium text-muted-foreground">
              {t(company ? "companySpaces" : "personalSpace")}
            </div>
            {list.map((space) => (
              <WorkspaceRow
                key={space.org_code}
                space={space}
                company={company}
                selected={selected}
                current={current}
                onSelect={onSelect}
                disabled={disabled}
              />
            ))}
          </div>
        )
      })}
    </div>
  )
}

function WorkspaceRow({
  space,
  company,
  selected,
  current,
  onSelect,
  disabled,
}: {
  space: IywWorkspace
  company: boolean
  selected?: string
  current?: string | null
  onSelect: (id: string) => void
  disabled: boolean
}) {
  const t = useTranslations("AccountFlow")
  return (
    <button
      key={space.org_code}
      type="button"
      disabled={disabled || !space.enabled}
      aria-pressed={selected === space.org_code || current === space.org_code}
      onClick={() => onSelect(space.org_code)}
      className={cn(
        "flex w-full min-w-0 items-center gap-3 rounded-lg px-2.5 py-3 text-left transition-colors disabled:opacity-40",
        selected === space.org_code || current === space.org_code
          ? "bg-primary/6"
          : "hover:bg-muted/60"
      )}
    >
      <span className="flex size-9 shrink-0 items-center justify-center rounded-xl border border-primary/5 bg-primary/5 text-primary/75">
        {company ? (
          <Building2 className="size-4" />
        ) : (
          <UserRound className="size-4" />
        )}
      </span>
      <WorkspaceRowCopy space={space} company={company} />
      {selected === space.org_code || current === space.org_code ? (
        <Check className="size-3.5 shrink-0 text-primary" />
      ) : null}
      {!space.enabled ? (
        <span className="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[9px] text-muted-foreground">
          {t("disabled")}
        </span>
      ) : null}
    </button>
  )
}

function WorkspaceRowCopy({
  space,
  company,
}: {
  space: IywWorkspace
  company: boolean
}) {
  const t = useTranslations("AccountFlow")
  return (
    <span className="min-w-0 flex-1">
      <span className="block truncate text-xs font-medium">
        {space.name || t("unnamedSpace")}
      </span>
      <span className="mt-1 block text-[10px] text-muted-foreground">
        {t(
          !space.enabled
            ? "spaceDisabled"
            : company
              ? space.is_own_create
                ? "ownCompany"
                : "companyMember"
              : "personalOnly"
        )}
      </span>
    </span>
  )
}
