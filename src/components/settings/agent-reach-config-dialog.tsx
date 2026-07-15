import { useState } from "react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import type {
  AgentReachChannel,
  AgentReachConfigKey,
  SupportedBrowser,
} from "@/lib/types"

const CONFIG_KEYS: AgentReachConfigKey[] = [
  "proxy",
  "github_token",
  "groq_key",
  "openai_key",
  "twitter_cookies",
  "youtube_cookies",
  "xhs_cookies",
]
const BROWSERS: SupportedBrowser[] = [
  "chrome",
  "edge",
  "firefox",
  "brave",
  "opera",
]
const CHANNELS: AgentReachChannel[] = [
  "twitter",
  "xiaoyuzhou",
  "xueqiu",
  "xiaohongshu",
  "reddit",
  "bilibili",
  "linkedin",
]

export interface AgentReachConfigDialogProps {
  open: boolean
  busy: boolean
  onOpenChange: (open: boolean) => void
  onSave: (key: AgentReachConfigKey, value: string) => Promise<void>
  onImportBrowser: (browser: SupportedBrowser) => Promise<void>
  onInstallChannels: (channels: AgentReachChannel[]) => Promise<void>
}

export function AgentReachConfigDialog(props: AgentReachConfigDialogProps) {
  const t = useTranslations("InternetToolsSettings.configDialog")
  const [key, setKey] = useState<AgentReachConfigKey>("proxy")
  const [value, setValue] = useState("")
  const [browser, setBrowser] = useState<SupportedBrowser>("chrome")
  const [channels, setChannels] = useState<AgentReachChannel[]>([])
  const labels: Record<AgentReachConfigKey, string> = {
    proxy: t("proxy"),
    github_token: t("githubToken"),
    groq_key: t("groqKey"),
    openai_key: t("openaiKey"),
    twitter_cookies: t("twitterCookies"),
    youtube_cookies: t("youtubeCookies"),
    xhs_cookies: t("xhsCookies"),
  }

  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="max-w-lg rounded-lg">
        <DialogHeader>
          <DialogTitle>{t("title")}</DialogTitle>
          <DialogDescription>{t("description")}</DialogDescription>
        </DialogHeader>

        <div className="space-y-3">
          <Select
            value={key}
            onValueChange={(next) => setKey(next as AgentReachConfigKey)}
          >
            <SelectTrigger className="w-full" aria-label={t("key")}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {CONFIG_KEYS.map((item) => (
                <SelectItem key={item} value={item}>
                  {labels[item]}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Input
            type="password"
            value={value}
            aria-label={t("value")}
            onChange={(event) => setValue(event.target.value)}
          />
          <Button
            className="w-full"
            disabled={props.busy || !value.trim()}
            onClick={() =>
              void props.onSave(key, value).then(() => setValue(""))
            }
          >
            {t("save")}
          </Button>
        </div>

        <div className="space-y-3 border-t pt-4">
          <h4 className="text-sm font-medium">{t("browser")}</h4>
          <div className="flex gap-2">
            <Select
              value={browser}
              onValueChange={(next) => setBrowser(next as SupportedBrowser)}
            >
              <SelectTrigger className="flex-1">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {BROWSERS.map((item) => (
                  <SelectItem key={item} value={item}>
                    {item}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Button
              variant="outline"
              disabled={props.busy}
              onClick={() => void props.onImportBrowser(browser)}
            >
              {t("import")}
            </Button>
          </div>
        </div>

        <div className="space-y-3 border-t pt-4">
          <h4 className="text-sm font-medium">{t("channels")}</h4>
          <div className="grid grid-cols-2 gap-2">
            {CHANNELS.map((channel) => (
              <label key={channel} className="flex items-center gap-2 text-sm">
                <Checkbox
                  checked={channels.includes(channel)}
                  onCheckedChange={(checked) =>
                    setChannels((current) =>
                      checked
                        ? [...current, channel]
                        : current.filter((item) => item !== channel)
                    )
                  }
                />
                {channel}
              </label>
            ))}
          </div>
        </div>

        <DialogFooter>
          <Button
            disabled={props.busy || channels.length === 0}
            onClick={() => void props.onInstallChannels(channels)}
          >
            {t("installChannels")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
