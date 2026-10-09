"use client"

import {
  AppWindow,
  ClipboardList,
  Layers,
  Monitor,
  MousePointerClick,
  PanelTop,
  RotateCw,
  ScreenShare,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import {
  SettingCard,
  SettingNote,
  SettingRow,
} from "@/components/computer/settings-layout"
import {
  SettingsError,
  SettingsSaveBar,
  SettingsSection,
} from "@/components/computer/settings-layout"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"

import { BlocklistRow } from "./blocklist-row"
import { StopShortcutRow } from "./stop-shortcut-row"
import { TTL_CHOICES } from "../computer-settings"
import { useComputerSettingsSectionState } from "./computer-settings-section-state"

export function ComputerSettingsSection() {
  const {
    t,
    tComputer,
    loaded,
    loading,
    saving,
    loadError,
    values,
    setValues,
    baseline,
    enabled,
    defaults,
    load,
    dirtyStopShortcut,
    dirty,
    editable,
    available,
    desktopHere,
    isLinux,
    save,
  } = useComputerSettingsSectionState()
  return (
    <SettingsSection
      icon={AppWindow}
      title={t("title")}
      description={t("description")}
    >
      {loadError && (
        <SettingsError>
          <span className="flex flex-wrap items-center gap-2">
            {t("loadFailed", { detail: loadError })}
            {!loaded && (
              <Button
                size="xs"
                variant="outline"
                onClick={() => void load()}
                disabled={loading}
              >
                <RotateCw className="size-3" />
                {tComputer("refresh")}
              </Button>
            )}
          </span>
        </SettingsError>
      )}

      <SettingCard>
        <SettingRow
          title={t("ttl.label")}
          description={t("ttl.hint")}
          htmlFor="computer-grant-ttl"
          control={
            <Select
              value={String(values.ttl)}
              onValueChange={(v) =>
                setValues((prev) => ({ ...prev, ttl: Number(v) }))
              }
              disabled={!editable}
            >
              <SelectTrigger id="computer-grant-ttl" size="sm" className="w-40">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {TTL_CHOICES.map((minutes) => (
                  <SelectItem key={minutes} value={String(minutes)}>
                    {minutes === 0
                      ? t("ttl.never")
                      : t("ttl.minutes", { minutes })}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          }
        />
        <BlocklistRow
          defaults={defaults}
          blocklist={values.blocklist}
          removed={values.removed}
          disabled={!editable}
          onChange={(blocklist, removed) =>
            setValues((prev) => ({ ...prev, blocklist, removed }))
          }
        />
        {desktopHere && (
          <StopShortcutRow
            value={loaded ? values.stopShortcut : null}
            saved={loaded && !dirtyStopShortcut ? baseline.stopShortcut : null}
            enabled={enabled}
            disabled={!editable}
            onChange={(stopShortcut) =>
              setValues((prev) => ({ ...prev, stopShortcut }))
            }
          />
        )}
        {desktopHere && (
          <SettingRow
            icon={PanelTop}
            title={t("strip.label")}
            description={t("strip.hint")}
            htmlFor="computer-show-indicator"
            control={
              <Switch
                id="computer-show-indicator"
                checked={values.showIndicator}
                onCheckedChange={(showIndicator) =>
                  setValues((prev) => ({ ...prev, showIndicator }))
                }
                disabled={!editable}
              />
            }
          />
        )}
      </SettingCard>

      {available && (
        <SettingCard>
          <SettingRow
            icon={Layers}
            title={t("foreground.label")}
            description={t("foreground.hint")}
            htmlFor="computer-allow-foreground"
            control={
              <Switch
                id="computer-allow-foreground"
                checked={values.allowForeground}
                onCheckedChange={(allowForeground) =>
                  setValues((prev) => ({ ...prev, allowForeground }))
                }
                disabled={!editable}
              />
            }
          />
          <SettingRow
            icon={MousePointerClick}
            title={t("delivery.label")}
            description={
              values.allowForeground
                ? t("delivery.hint")
                : t("delivery.hintOff")
            }
            htmlFor="computer-default-delivery"
            control={
              <Select
                value={
                  values.allowForeground ? values.defaultDelivery : "background"
                }
                onValueChange={(v) =>
                  setValues((prev) => ({
                    ...prev,
                    defaultDelivery:
                      v === "foreground" ? "foreground" : "background",
                  }))
                }
                disabled={!editable || !values.allowForeground}
              >
                <SelectTrigger
                  id="computer-default-delivery"
                  size="sm"
                  className="w-40"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="background">
                    {t("delivery.background")}
                  </SelectItem>
                  <SelectItem value="foreground">
                    {t("delivery.foreground")}
                  </SelectItem>
                </SelectContent>
              </Select>
            }
          />
        </SettingCard>
      )}

      {available && (
        <SettingCard>
          <SettingRow
            icon={AppWindow}
            title={t("launch.label")}
            description={t("launch.hint")}
            htmlFor="computer-launch-enabled"
            control={
              <Switch
                id="computer-launch-enabled"
                checked={values.launchEnabled}
                onCheckedChange={(launchEnabled) =>
                  setValues((prev) => ({ ...prev, launchEnabled }))
                }
                disabled={!editable}
              />
            }
          />
          <SettingRow
            icon={ClipboardList}
            title={t("clipboard.label")}
            description={t("clipboard.hint")}
            htmlFor="computer-clipboard-enabled"
            control={
              <Switch
                id="computer-clipboard-enabled"
                checked={values.clipboardEnabled}
                onCheckedChange={(clipboardEnabled) =>
                  setValues((prev) => ({ ...prev, clipboardEnabled }))
                }
                disabled={!editable}
              />
            }
          />
          {!isLinux && (
            <SettingRow
              icon={ScreenShare}
              title={t("screen.label")}
              description={t("screen.hint")}
              htmlFor="computer-screen-enabled"
              control={
                <Switch
                  id="computer-screen-enabled"
                  checked={values.screenEnabled}
                  onCheckedChange={(screenEnabled) =>
                    setValues((prev) => ({ ...prev, screenEnabled }))
                  }
                  disabled={!editable}
                />
              }
            />
          )}
        </SettingCard>
      )}

      <SettingNote icon={Monitor}>{t("boundary")}</SettingNote>

      <SettingsSaveBar
        onSave={() => void save()}
        saving={saving}
        disabled={!editable || !dirty}
        label={t("save")}
        savingLabel={t("saving")}
      />
    </SettingsSection>
  )
}
