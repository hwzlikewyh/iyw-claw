"use client"

import type { ReactNode } from "react"
import {
  Check,
  MessageSquareMore,
  Monitor,
  Moon,
  Palette,
  PanelLeft,
  Sun,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { useTheme } from "next-themes"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { useSidebarViewOptions } from "@/contexts/sidebar-view-options-context"
import { useConversationDisplayPreferences } from "@/contexts/conversation-display-context"
import type { ConversationDisplayMode } from "@/lib/conversation-display-preferences"
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
import {
  SettingsPageLayout,
  SettingsPageHeader,
  SettingSection,
  SettingRow,
  SettingSectionBody,
} from "@/components/settings/settings-ui"

type ThemeMode = "system" | "light" | "dark"

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
              "text-xs font-medium transition-[background-color,color,box-shadow]",
              active
                ? "bg-primary text-primary-foreground shadow-sm"
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
  const tShell = useTranslations("SettingsShell")
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
  const {
    mode: conversationDisplayMode,
    collapseCompletedTurn,
    autoOpenErrors,
    setMode: setConversationDisplayMode,
    setCollapseCompletedTurn,
    setAutoOpenErrors,
  } = useConversationDisplayPreferences()

  const resolvedThemeLabel =
    resolvedTheme === "dark"
      ? t("resolvedTheme.dark")
      : resolvedTheme === "light"
        ? t("resolvedTheme.light")
        : t("resolvedTheme.unknown")

  return (
    <SettingsPageLayout>
      <SettingsPageHeader
        icon={Palette}
        title={tShell("nav.appearance")}
        description={t("sectionDescription")}
      />

      {/* Theme section */}
      <SettingSection
        icon={Sun}
        title={t("sectionTitle")}
        description={t("sectionDescription")}
      >
        {/* Theme mode row */}
        <SettingSectionBody>
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

          {/* Theme color picker */}
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
                      "flex items-center gap-2 rounded-md border px-3 py-2 text-xs",
                      "transition-[background-color,border-color,color,box-shadow]",
                      "hover:bg-accent hover:text-accent-foreground",
                      isActive &&
                        "border-primary bg-primary/[0.06] text-foreground shadow-sm ring-1 ring-primary/20"
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
                    {isActive ? (
                      <Check
                        className="ml-auto size-3.5 shrink-0 text-primary"
                        aria-hidden
                      />
                    ) : null}
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

          {/* Zoom level */}
          <div className="border-t pt-4">
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
                <SelectTrigger className="w-44">
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
            </SettingRow>
            <p className="px-0 text-[11px] text-muted-foreground">
              {t("zoomLevel.current", { zoom: zoomLevel })}
            </p>
          </div>
        </SettingSectionBody>
      </SettingSection>

      {/* Sidebar section */}
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

        <SettingSectionBody className="border-t pt-4">
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
        </SettingSectionBody>
      </SettingSection>

      {/* Conversation rendering section */}
      <SettingSection
        icon={MessageSquareMore}
        title={t("conversation.sectionTitle")}
        description={t("conversation.sectionDescription")}
      >
        <SettingSectionBody>
          <SettingRow
            title={t("conversation.displayModeTitle")}
            description={t("conversation.displayModeDescription")}
          >
            <SegmentedControl<ConversationDisplayMode>
              value={conversationDisplayMode}
              onChange={setConversationDisplayMode}
              options={[
                {
                  value: "summary",
                  label: t("conversation.modeSummary"),
                },
                {
                  value: "full",
                  label: t("conversation.modeFull"),
                },
                {
                  value: "minimal",
                  label: t("conversation.modeMinimal"),
                },
              ]}
            />
          </SettingRow>
          <SettingRow
            title={t("conversation.collapseCompletedTitle")}
            description={t("conversation.collapseCompletedDescription")}
          >
            <Switch
              checked={collapseCompletedTurn}
              onCheckedChange={setCollapseCompletedTurn}
              aria-label={t("conversation.collapseCompletedTitle")}
            />
          </SettingRow>
          <SettingRow
            title={t("conversation.autoOpenErrorsTitle")}
            description={t("conversation.autoOpenErrorsDescription")}
          >
            <Switch
              checked={autoOpenErrors}
              onCheckedChange={setAutoOpenErrors}
              aria-label={t("conversation.autoOpenErrorsTitle")}
            />
          </SettingRow>
        </SettingSectionBody>
      </SettingSection>

      {/* Font section */}
      <div className="[&_section]:rounded-xl">
        <FontSettingsSection />
      </div>
    </SettingsPageLayout>
  )
}
