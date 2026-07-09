"use client"

import type { ComponentType, ReactNode } from "react"
import { Monitor, Moon, PanelLeft, Sun } from "lucide-react"
import { useTranslations } from "next-intl"
import { useTheme } from "next-themes"
import { ScrollArea } from "@/components/ui/scroll-area"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { useSidebarViewOptions } from "@/contexts/sidebar-view-options-context"
import { useThemeColor, useZoomLevel } from "@/hooks/use-appearance"
import { cn } from "@/lib/utils"
import type {
  SidebarSectionOrder,
  SidebarSortMode,
} from "@/lib/sidebar-view-mode-storage"
import {
  DEFAULT_ZOOM_LEVEL,
  THEME_COLOR_PREVIEW,
  THEME_COLORS,
  ZOOM_LEVELS,
  type ThemeColor,
  type ZoomLevel,
} from "@/lib/theme-presets"
import { FontSettingsSection } from "./font-settings-section"

type ThemeMode = "system" | "light" | "dark"

function SettingSection({
  icon: Icon,
  title,
  description,
  children,
}: {
  icon: ComponentType<{ className?: string }>
  title: string
  description: string
  children: ReactNode
}) {
  return (
    <section className="rounded-lg border bg-card">
      <div className="border-b px-4 py-3">
        <div className="flex items-center gap-2">
          <Icon className="h-4 w-4 text-muted-foreground" />
          <h2 className="text-sm font-semibold">{title}</h2>
        </div>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">
          {description}
        </p>
      </div>
      <div className="space-y-4 p-4">{children}</div>
    </section>
  )
}

function SettingRow({
  title,
  description,
  children,
}: {
  title: string
  description?: string
  children: ReactNode
}) {
  return (
    <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_auto] md:items-center">
      <div className="min-w-0">
        <div className="text-sm font-medium">{title}</div>
        {description ? (
          <p className="mt-1 text-xs leading-5 text-muted-foreground">
            {description}
          </p>
        ) : null}
      </div>
      <div className="md:justify-self-end">{children}</div>
    </div>
  )
}

function SegmentedControl<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T
  options: Array<{ value: T; label: string; icon?: ReactNode }>
  onChange: (value: T) => void
}) {
  return (
    <div className="inline-grid rounded-lg border bg-muted/40 p-1 sm:auto-cols-fr sm:grid-flow-col">
      {options.map((option) => {
        const active = value === option.value
        return (
          <button
            key={option.value}
            type="button"
            onClick={() => onChange(option.value)}
            aria-pressed={active}
            className={cn(
              "inline-flex h-8 min-w-28 items-center justify-center gap-1.5 rounded-md px-3",
              "text-xs font-medium transition-colors",
              active
                ? "bg-background text-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground"
            )}
          >
            {option.icon}
            {option.label}
          </button>
        )
      })}
    </div>
  )
}

export function AppearanceSettings() {
  const t = useTranslations("AppearanceSettings")
  const { theme, resolvedTheme, setTheme } = useTheme()
  const { themeColor, setThemeColor } = useThemeColor()
  const { zoomLevel, setZoomLevel } = useZoomLevel()
  const {
    showCompleted,
    setShowCompleted,
    sortMode,
    setSortMode,
    sectionOrder,
    setSectionOrder,
  } = useSidebarViewOptions()

  const resolvedThemeLabel =
    resolvedTheme === "dark"
      ? t("resolvedTheme.dark")
      : resolvedTheme === "light"
        ? t("resolvedTheme.light")
        : t("resolvedTheme.unknown")

  return (
    <ScrollArea className="h-full">
      <div className="w-full max-w-5xl space-y-4 p-3 md:p-4">
        <SettingSection
          icon={Sun}
          title={t("sectionTitle")}
          description={t("sectionDescription")}
        >
          <SettingRow title={t("themeMode")}>
            <SegmentedControl<ThemeMode>
              value={(theme ?? "system") as ThemeMode}
              onChange={(value) => {
                setTheme(value)
                if (
                  typeof window !== "undefined" &&
                  "__TAURI_INTERNALS__" in window
                ) {
                  import("@/lib/tauri").then((t) =>
                    t.updateAppearanceMode(value).catch(() => {})
                  )
                }
              }}
              options={[
                {
                  value: "system",
                  label: t("system"),
                  icon: <Monitor className="h-3.5 w-3.5" />,
                },
                {
                  value: "light",
                  label: t("light"),
                  icon: <Sun className="h-3.5 w-3.5" />,
                },
                {
                  value: "dark",
                  label: t("dark"),
                  icon: <Moon className="h-3.5 w-3.5" />,
                },
              ]}
            />
            <p
              className="mt-2 text-right text-[11px] text-muted-foreground"
              suppressHydrationWarning
            >
              {t("currentTheme", { theme: resolvedThemeLabel })}
            </p>
          </SettingRow>

          <div className="border-t pt-4">
            <div className="mb-3">
              <div className="text-sm font-medium">
                {t("themeColor.sectionTitle")}
              </div>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">
                {t("themeColor.sectionDescription")}
              </p>
            </div>
            <div className="grid grid-cols-3 gap-2 sm:grid-cols-4 md:grid-cols-6">
              {THEME_COLORS.map((color) => {
                const isActive = themeColor === color
                return (
                  <button
                    key={color}
                    type="button"
                    onClick={() => setThemeColor(color as ThemeColor)}
                    aria-pressed={isActive}
                    className={cn(
                      "flex items-center gap-2 rounded-md border px-3 py-2 text-xs transition-colors",
                      "hover:bg-accent hover:text-accent-foreground",
                      isActive && "border-primary ring-2 ring-primary/30"
                    )}
                  >
                    <span
                      className="size-4 shrink-0 rounded-full border"
                      style={{ backgroundColor: THEME_COLOR_PREVIEW[color] }}
                      aria-hidden
                    />
                    <span className="truncate">
                      {t(`themeColor.options.${color}`)}
                    </span>
                  </button>
                )
              })}
            </div>

            <p className="mt-2 text-[11px] text-muted-foreground">
              {t("themeColor.current", {
                color: t(`themeColor.options.${themeColor}`),
              })}
            </p>
          </div>

          <SettingRow
            title={t("zoomLevel.sectionTitle")}
            description={t("zoomLevel.sectionDescription")}
          >
            <Select
              value={String(zoomLevel)}
              onValueChange={(value) =>
                setZoomLevel(parseInt(value, 10) as ZoomLevel)
              }
            >
              <SelectTrigger className="w-56">
                <SelectValue placeholder={t("zoomLevel.placeholder")} />
              </SelectTrigger>
              <SelectContent align="start">
                {ZOOM_LEVELS.map((z) => (
                  <SelectItem key={z} value={String(z)}>
                    {z}%
                    {z === DEFAULT_ZOOM_LEVEL
                      ? ` (${t("zoomLevel.default")})`
                      : ""}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <p className="text-[11px] text-muted-foreground">
              {t("zoomLevel.current", { zoom: zoomLevel })}
            </p>
          </SettingRow>
        </SettingSection>

        <SettingSection
          icon={PanelLeft}
          title={t("sidebar.sectionTitle")}
          description={t("sidebar.sectionDescription")}
        >
          <SettingRow
            title={t("sidebar.showCompletedTitle")}
            description={t("sidebar.showCompletedDescription")}
          >
            <Switch
              checked={showCompleted}
              onCheckedChange={setShowCompleted}
              aria-label={t("sidebar.showCompletedTitle")}
            />
          </SettingRow>

          <SettingRow
            title={t("sidebar.sortModeTitle")}
            description={t("sidebar.sortModeDescription")}
          >
            <SegmentedControl<SidebarSortMode>
              value={sortMode}
              onChange={setSortMode}
              options={[
                {
                  value: "created",
                  label: t("sidebar.sortByCreatedAt"),
                },
                {
                  value: "updated",
                  label: t("sidebar.sortByUpdatedAt"),
                },
              ]}
            />
          </SettingRow>

          <SettingRow
            title={t("sidebar.sectionOrderTitle")}
            description={t("sidebar.sectionOrderDescription")}
          >
            <SegmentedControl<SidebarSectionOrder>
              value={sectionOrder}
              onChange={setSectionOrder}
              options={[
                {
                  value: "folders-first",
                  label: t("sidebar.sectionOrderFoldersFirst"),
                },
                {
                  value: "chats-first",
                  label: t("sidebar.sectionOrderChatsFirst"),
                },
              ]}
            />
          </SettingRow>
        </SettingSection>

        <div className="[&_section]:rounded-lg">
          <FontSettingsSection />
        </div>
      </div>
    </ScrollArea>
  )
}
