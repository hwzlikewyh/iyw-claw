"use client"

import { useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { mcpGetMarketplaceServerDetail } from "@/lib/api"
import { extractAppCommandError } from "@/lib/app-error"
import {
  beginConnectorInstall,
  cancelConnectorInstall,
  connectorTaskActive,
  type ConnectorInstallTask,
} from "@/lib/connector-install-tasks"
import type {
  McpMarketplaceItem,
  McpMarketplaceServerDetail,
} from "@/lib/types"
import {
  ConnectorParameterError,
  connectorParameterValues,
} from "./connector-install-fields"

function useConnectorDetail(
  item: McpMarketplaceItem,
  preferredOption?: string | null
) {
  const [detail, setDetail] = useState<McpMarketplaceServerDetail | null>(null)
  const [optionId, setOptionId] = useState("")
  const [error, setError] = useState("")
  const [revision, setRevision] = useState(0)
  useEffect(() => {
    let active = true
    mcpGetMarketplaceServerDetail({
      providerId: item.provider_id,
      serverId: item.server_id,
    })
      .then((value) => {
        if (!active) return
        setDetail(value)
        setOptionId(
          value.install_options.find((option) => option.id === preferredOption)
            ?.id ??
            value.default_option_id ??
            value.install_options[0]?.id ??
            ""
        )
        setError("")
      })
      .catch((cause) => {
        if (active)
          setError(extractAppCommandError(cause)?.message || String(cause))
      })
    return () => {
      active = false
    }
  }, [item.provider_id, item.server_id, preferredOption, revision])
  const option = detail?.install_options.find((value) => value.id === optionId)
  return {
    detail,
    option,
    optionId,
    setOptionId,
    error,
    reload: () => setRevision((value) => value + 1),
  }
}

type SubmissionOptions = {
  item: McpMarketplaceItem
  data: ReturnType<typeof useConnectorDetail>
  onTaskChanged: () => void
  onAccepted: () => void
}

function useInstallSubmission(options: SubmissionOptions) {
  const { item, data, onTaskChanged, onAccepted } = options
  const t = useTranslations("ConnectorMarket")
  const [task, setTask] = useState<ConnectorInstallTask>()
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState("")
  const inFlight = useRef(false)
  const install = async (values: Record<string, string>) => {
    if (!data.option || inFlight.current) return false
    inFlight.current = true
    setSubmitting(true)
    setError("")
    try {
      const parameters = connectorParameterValues(
        data.option.parameters,
        values
      )
      const started = await beginConnectorInstall({
        providerId: item.provider_id,
        serverId: item.server_id,
        optionId: data.option.id,
        protocol: data.option.protocol,
        parameterValues: parameters,
      })
      setTask(started)
      onAccepted()
      onTaskChanged()
      return true
    } catch (cause) {
      setError(
        cause instanceof ConnectorParameterError
          ? t(
              cause.reason === "required"
                ? "parameterRequired"
                : "parameterInvalid",
              { field: cause.message }
            )
          : extractAppCommandError(cause)?.message || String(cause)
      )
      return false
    } finally {
      inFlight.current = false
      setSubmitting(false)
    }
  }
  return { task, setTask, submitting, error, setError, inFlight, install }
}

export function useConnectorInstall({
  item,
  task: externalTask,
  onTaskChanged,
}: {
  item: McpMarketplaceItem
  task?: ConnectorInstallTask
  onTaskChanged: () => void
}) {
  const data = useConnectorDetail(item, externalTask?.optionId)
  const [values, setValues] = useState<Record<string, string>>({})
  const operation = useInstallSubmission({
    item,
    data,
    onTaskChanged,
    onAccepted: () => setValues({}),
  })
  const task =
    externalTask &&
    (!operation.task ||
      externalTask.id === operation.task.id ||
      externalTask.createdAt > operation.task.createdAt)
      ? externalTask
      : operation.task
  const busy =
    operation.submitting || Boolean(task && connectorTaskActive(task))
  return {
    ...data,
    values,
    task,
    busy,
    cancel: () => cancelSubmission(operation, task, onTaskChanged),
    error: operation.error || data.error,
    install: () => {
      return busy ? Promise.resolve(false) : operation.install(values)
    },
    setOption: (value: string) => {
      data.setOptionId(value)
      setValues({})
    },
    setValue: (key: string, value: string) =>
      setValues((current) => ({ ...current, [key]: value })),
  }
}

async function cancelSubmission(
  operation: ReturnType<typeof useInstallSubmission>,
  task: ConnectorInstallTask | undefined,
  onTaskChanged: () => void
) {
  if (!task?.cancellable || operation.inFlight.current) return
  operation.inFlight.current = true
  try {
    operation.setTask(await cancelConnectorInstall(task.id))
    onTaskChanged()
  } catch (cause) {
    operation.setError(extractAppCommandError(cause)?.message || String(cause))
  } finally {
    operation.inFlight.current = false
  }
}
