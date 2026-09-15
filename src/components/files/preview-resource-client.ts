import {
  getTransport,
  getShellTransport,
  getServerBaseUrl,
  getActiveRemoteConnectionId,
} from "@/lib/transport"

export interface PreviewResource {
  id: string
  url: string
  size: number
}
export interface ResourceState {
  key: string
  resource?: PreviewResource
  error?: string
}
const RENEW_INTERVAL_MS = 30_000

export function acquireResource(
  options: { rootPath: string; path: string; key: string },
  update: (state: ResourceState) => void
) {
  const connectionId = getActiveRemoteConnectionId()
  const transport = connectionId === null ? getTransport() : getShellTransport()
  const command = (verb: string) =>
    `${verb}_${connectionId === null ? "" : "remote_"}preview_resource`
  const base = getServerBaseUrl()
  const state = {
    disposed: false,
    resource: undefined as PreviewResource | undefined,
    stopRenew: () => {},
  }
  const release = (resource: PreviewResource) => {
    void transport.call(command("close"), { id: resource.id }).catch(() => {
      console.warn(
        "[preview] release failed; lease expiry will reclaim resource"
      )
    })
  }
  void transport
    .call<PreviewResource>(command("open"), {
      rootPath: options.rootPath,
      path: options.path,
      ...(connectionId === null ? {} : { connectionId }),
    })
    .then((value) => {
      if (state.disposed) {
        release(value)
        return
      }
      state.resource = {
        ...value,
        url: value.url.startsWith("/") ? `${base}${value.url}` : value.url,
      }
      update({ key: options.key, resource: state.resource })
      state.stopRenew = renewResource(
        () => transport.call(command("renew"), { id: value.id }),
        () => {
          if (state.disposed) return
          release(value)
          update({ key: options.key, error: "Preview connection expired" })
        }
      )
    })
    .catch((error: unknown) => {
      if (!state.disposed)
        update({
          key: options.key,
          error:
            error instanceof Error ? error.message : "Unable to open preview",
        })
    })
  return () => {
    state.disposed = true
    state.stopRenew()
    if (state.resource) release(state.resource)
    state.resource = undefined
  }
}

function renewResource(renew: () => Promise<unknown>, onError: () => void) {
  let inFlight = false
  const timer = setInterval(() => {
    if (inFlight) return
    inFlight = true
    void renew()
      .catch(() => {
        clearInterval(timer)
        onError()
      })
      .finally(() => {
        inFlight = false
      })
  }, RENEW_INTERVAL_MS)
  return () => clearInterval(timer)
}
