"use client"

import { RotateCcw, SlidersHorizontal } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import type { Scenario, ScenarioCategory } from "@/lib/types"
import type {
  ScenarioPreference,
  ScenarioPreferences,
} from "./scenario-preferences"

interface Props {
  categories: ScenarioCategory[]
  scenarios: Scenario[]
  preferences: ScenarioPreferences
  onUpdate: (id: string, patch: Partial<ScenarioPreference>) => void
  onReset: (id: string) => void
}

export function ScenarioPreferencesDialog(props: Props) {
  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button variant="ghost" size="sm" className="shrink-0">
          <SlidersHorizontal className="size-4" />
          管理场景
        </Button>
      </DialogTrigger>
      <DialogContent className="max-w-3xl">
        <DialogHeader>
          <DialogTitle>我的场景设置</DialogTitle>
          <DialogDescription>
            官方内容不会被修改；这里的指令、排序和隐藏设置仅保存在当前客户端。
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-5">
          {props.categories.map((category) => {
            const items = props.scenarios.filter(
              (scenario) => scenario.categoryKey === category.key
            )
            if (!items.length) return null
            return (
              <section key={category.key} className="space-y-2">
                <h3 className="text-sm font-semibold">
                  {category.displayName}
                </h3>
                {items.map((scenario) => (
                  <PreferenceRow
                    key={scenario.id}
                    scenario={scenario}
                    preference={props.preferences[scenario.id]}
                    onUpdate={(patch) => props.onUpdate(scenario.id, patch)}
                    onReset={() => props.onReset(scenario.id)}
                  />
                ))}
              </section>
            )
          })}
        </div>
      </DialogContent>
    </Dialog>
  )
}

function PreferenceRow({
  scenario,
  preference,
  onUpdate,
  onReset,
}: {
  scenario: Scenario
  preference?: ScenarioPreference
  onUpdate: (patch: Partial<ScenarioPreference>) => void
  onReset: () => void
}) {
  return (
    <div className="rounded-lg border bg-card p-3">
      <div className="flex items-center gap-3">
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium">{scenario.displayName}</p>
          <p className="truncate text-xs text-muted-foreground">
            {scenario.skillPackageSlug}@{scenario.skillPackageVersion}
          </p>
        </div>
        <label className="flex items-center gap-2 text-xs text-muted-foreground">
          显示
          <Switch
            checked={!preference?.hidden}
            onCheckedChange={(checked) => onUpdate({ hidden: !checked })}
          />
        </label>
        <Input
          className="w-20"
          type="number"
          min={0}
          aria-label={`${scenario.displayName} 排序`}
          value={preference?.sortOrder ?? scenario.sortOrder}
          onChange={(event) =>
            onUpdate({ sortOrder: Number(event.target.value) || 0 })
          }
        />
        <Button
          type="button"
          size="icon-sm"
          variant="ghost"
          title="恢复官方设置"
          onClick={onReset}
        >
          <RotateCcw className="size-4" />
        </Button>
      </div>
      <Textarea
        className="mt-3 min-h-24 resize-y text-xs"
        aria-label={`${scenario.displayName} 指令`}
        value={preference?.promptOverride ?? ""}
        placeholder={scenario.promptTemplate}
        maxLength={32000}
        onChange={(event) => onUpdate({ promptOverride: event.target.value })}
      />
    </div>
  )
}
