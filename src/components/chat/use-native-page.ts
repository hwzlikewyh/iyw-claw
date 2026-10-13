import { useTranslations } from "next-intl"
import { useEffect } from "react"
import { isDesktop } from "@/lib/platform"
import { useNativePageSurface } from "./use-native-page-surface"
import { useNativeEvents } from "./use-native-page-events"
import {
  useNativePageModel,
  type NativePageProps,
} from "./use-native-page-model"
import { useNativePageHandoff } from "./use-native-page-handoff"
import { nativePageActions } from "./native-page-actions"
import { nativePageCommand } from "@/lib/native-page-api"
const NATIVE_PAGE_LOAD_TIMEOUT_MS = 30000

export function useNativePage(props: NativePageProps) {
  const t = useTranslations("Folder.chat.interactiveHtml.native")
  const model = useNativePageModel()
  useNativePageLifecycle(props, model)
  const handoff = useNativePageHandoff(props.tabId, model)
  useNativeEvents({
    props,
    setState: model.setState,
    setError: model.setError,
    loadTimeout: t("loadTimeout"),
    setPicking: model.setPicking,
    setErrors: model.setErrors,
    setFind: model.setFind,
    sendText: handoff.sendText,
    setDownloads: model.setDownloads,
  })
  useNativePageSurface(model.surface, {
    id: props.id,
    url: props.url,
    visible: props.visible && !model.capture,
    revision: model.revision,
    onState: model.setState,
    onError: model.setError,
  })
  const actions = nativePageActions({
    props,
    model,
    handoff,
    noErrors: t("noErrors"),
    invalidUrl: t("invalidUrl"),
  })
  return {
    ...model,
    ...actions,
    ...handoff,
    native: isDesktop(),
    url: model.state?.url ?? props.url,
    props,
  }
}
export type NativePageController = Omit<
  ReturnType<typeof useNativePage>,
  "surface"
>

function useNativePageLifecycle(
  props: NativePageProps,
  {
    state,
    picking,
    setPicking,
    setError,
  }: ReturnType<typeof useNativePageModel>
) {
  const t = useTranslations("Folder.chat.interactiveHtml.native")
  useEffect(() => {
    if (!state?.loading) return
    const timer = window.setTimeout(
      () => setError(t("loadTimeout")),
      NATIVE_PAGE_LOAD_TIMEOUT_MS
    )
    return () => clearTimeout(timer)
  }, [state?.loading, setError, t])
  useEffect(() => {
    if (!isDesktop() || props.visible || !picking) return
    void nativePageCommand(props.id, "cancel-pick")
      .then(() => setPicking(false))
      .catch(() => setPicking(false))
  }, [props.id, props.visible, picking, setPicking])
}
