import { useCallback, useEffect, useRef, useState } from "react"
import { extractAppCommandError } from "@/lib/app-error"
import {
  connectorUserRequest,
  importConnector,
  newOwnedConnector,
  type OwnedConnector,
} from "@/lib/connector-publishing"
import {
  uploadConnectorArtifact,
  type PendingConnectorUpload,
} from "@/lib/connector-publishing-upload"

export function useConnectorPublishing(onChanged: () => void) {
  const [draft, setDraft] = useState<OwnedConnector | null>(null)
  const [dirty, setDirty] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState("")
  const [status, setStatus] = useState("")
  const active = useRef(true),
    running = useRef(false)
  const cancellation = useRef<AbortController | null>(null)
  const currentUpload = useRef<PendingConnectorUpload | null>(null)
  const pending = useRef(new Map<number, PendingConnectorUpload>())
  const [uploading, setUploading] = useState<number | null>(null)
  const [uploadPhase, setUploadPhase] = useState<
    "digest" | "transfer" | "verify" | null
  >(null)
  const [editorSession, setEditorSession] = useState(0)
  const retainPendingUpload = (
    index: number,
    value: PendingConnectorUpload
  ) => {
    pending.current.set(index, value)
    currentUpload.current = value
    if (!active.current) {
      void connectorUserRequest("uploads/abort", {
        uploadToken: value.task.uploadToken,
      }).catch(() => {})
    }
  }
  useEffect(() => {
    active.current = true
    return () => {
      active.current = false
      cancellation.current?.abort()
      if (currentUpload.current) {
        void connectorUserRequest("uploads/abort", {
          uploadToken: currentUpload.current.task.uploadToken,
        }).catch(() => {})
      }
    }
  }, [])
  useEffect(() => {
    const prevent = (event: BeforeUnloadEvent) => {
      if (dirty || busy) {
        event.preventDefault()
        event.returnValue = ""
      }
    }
    window.addEventListener("beforeunload", prevent)
    return () => window.removeEventListener("beforeunload", prevent)
  }, [busy, dirty])
  const run = async (work: () => Promise<void>) => {
    if (running.current) return
    running.current = true
    setBusy(true)
    setError("")
    try {
      await work()
    } catch (cause) {
      if (active.current)
        setError(
          extractAppCommandError(cause)?.message ||
            (cause instanceof Error ? cause.message : String(cause))
        )
    } finally {
      running.current = false
      if (active.current) {
        setBusy(false)
        setUploading(null)
        setUploadPhase(null)
      }
    }
  }
  const canDiscard = useCallback(
    () =>
      !running.current &&
      (!dirty || window.confirm("当前连接器有未保存修改，确认放弃？")),
    [dirty]
  )
  const change = (next: OwnedConnector) => {
    setDraft(next)
    setDirty(true)
    setStatus("")
  }
  const create = () => {
    if (canDiscard()) {
      setDraft(newOwnedConnector())
      setEditorSession((value) => value + 1)
      setDirty(false)
      setError("")
      setStatus("")
      return true
    }
    return false
  }
  const close = () => {
    if (canDiscard()) {
      setDraft(null)
      setDirty(false)
      setError("")
      setStatus("")
    }
  }
  const open = (id: string) => {
    if (canDiscard())
      void run(async () => {
        const value = await connectorUserRequest<OwnedConnector>(
          "detail-owned",
          { server_id: id }
        )
        if (active.current) {
          setDraft(value)
          setEditorSession((current) => current + 1)
          setDirty(false)
          setStatus("")
        }
      })
  }
  const readDeclaration = (file: File) => {
    if (canDiscard())
      void run(async () => {
        if (file.size > 256 * 1024)
          throw Error("连接器声明不能超过256 KiB，组件文件另行上传")
        const value = importConnector(await file.text())
        if (active.current) {
          setDraft(value)
          setEditorSession((current) => current + 1)
          setDirty(true)
          setStatus("已导入声明，检查后保存；尚未发布")
        }
      })
  }
  const save = () => {
    if (!draft) return
    void run(async () => {
      const value = await connectorUserRequest<OwnedConnector>("save", draft)
      if (active.current) {
        setDraft(value)
        setDirty(false)
        setStatus("草稿已保存并回读；发布后才供其他用户选择")
        onChanged()
      }
    })
  }
  const publish = (enabled: boolean) => {
    if (!draft) return
    void run(async () => {
      if (dirty || !draft.revision)
        throw Error("请先保存当前草稿，再发布或下架")
      if (
        !window.confirm(
          enabled
            ? "发布到公开连接器市场？其他用户可按需下载使用，此版本发布内容不可覆盖。"
            : "下架此连接器？新安装将停止，已安装版本保留。"
        )
      )
        return
      const value = await connectorUserRequest<OwnedConnector>(
        enabled ? "publish" : "unpublish",
        { server_id: draft.server_id, revision: draft.revision }
      )
      if (active.current) {
        setDraft(value)
        setDirty(false)
        setStatus(enabled ? "已发布并回读" : "已下架并回读")
        onChanged()
      }
    })
  }
  const upload = (file: File, index: number) => {
    if (!draft) return
    void run(async () => {
      const controller = new AbortController()
      cancellation.current = controller
      setUploading(index)
      const artifact = await uploadConnectorArtifact(file, {
        signal: controller.signal,
        pending: pending.current.get(index),
        retain: (value) => retainPendingUpload(index, value),
        onPhase: setUploadPhase,
        status: (text) => {
          if (active.current) setStatus(text)
        },
      })
      controller.signal.throwIfAborted()
      if (active.current) {
        const next = structuredClone(draft)
        next.install_options[index].artifact_id = artifact.id
        change(next)
        setStatus(`文件已校验并登记：${artifact.filename}；请保存草稿`)
        pending.current.delete(index)
        currentUpload.current = null
      }
    })
  }
  const cancelUpload = async () => {
    if (uploadPhase === "verify") return
    cancellation.current?.abort()
    const task = uploading == null ? undefined : pending.current.get(uploading)
    if (task) {
      const deadline = Date.now() + 5_000
      while (running.current && Date.now() < deadline)
        await new Promise((resolve) => setTimeout(resolve, 25))
      await connectorUserRequest("uploads/abort", {
        uploadToken: task.task.uploadToken,
      }).catch(() => {})
    }
    if (uploading != null) pending.current.delete(uploading)
    currentUpload.current = null
  }
  return {
    draft,
    editorSession,
    dirty,
    busy,
    error,
    status,
    uploading,
    uploadPhase,
    change,
    create,
    close,
    open,
    readDeclaration,
    save,
    publish,
    upload,
    cancelUpload,
    canDiscard,
  }
}
