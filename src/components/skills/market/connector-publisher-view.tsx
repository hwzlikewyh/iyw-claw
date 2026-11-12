import { useEffect, useRef } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { useConnectorPublishing } from "./use-connector-publishing"
import { useOwnedConnectors } from "./use-owned-connectors"
import {
  useMarketViewAction,
  type MarketViewAction,
} from "./market-view-action"
import { CapabilityCard, CAPABILITY_GRID_CLASS } from "./capability-card"
import { MarketPagination } from "./market-pagination"
import { useMarketPageFocus } from "./use-market-page-focus"
import { ConnectorPublishForm } from "./connector-publish-form"

export function ConnectorPublisherView({
  action,
  onBrowse,
  onLeaveRequest,
}: {
  action?: MarketViewAction
  onBrowse: () => void
  onLeaveRequest: (canLeave: () => boolean) => void
}) {
  const catalog = useOwnedConnectors()
  const editor = useConnectorPublishing(catalog.refresh)
  const upload = useRef<HTMLInputElement>(null)
  const root = useMarketPageFocus({
    ...catalog.pagination,
    enabled: !editor.draft,
    error: catalog.error,
  })
  useMarketViewAction(action, (kind) => {
    if (kind === "upload") editor.create()
    else if (kind === "mine") editor.close()
  })
  useEffect(() => {
    onLeaveRequest(editor.canDiscard)
    return () => onLeaveRequest(() => true)
  }, [editor.canDiscard, onLeaveRequest])
  return (
    <div ref={root} className="@container min-w-0 py-4">
      <div className="mb-4 flex flex-wrap items-center gap-2">
        <h2 className="mr-auto text-base font-semibold">
          {editor.draft ? "创建 / 编辑连接器" : "我的连接器上传"}
        </h2>
        <Button
          variant="outline"
          disabled={editor.busy}
          onClick={() => (editor.draft ? editor.close() : onBrowse())}
        >
          {editor.draft ? "返回我的上传" : "返回市场"}
        </Button>
        <Button
          variant="outline"
          disabled={editor.busy}
          onClick={() => upload.current?.click()}
        >
          导入声明 JSON
        </Button>
        <Button disabled={editor.busy} onClick={editor.create}>
          新建连接器
        </Button>
        <input
          ref={upload}
          type="file"
          accept=".json,application/json"
          className="hidden"
          onChange={(event) => {
            const file = event.target.files?.[0]
            if (file) editor.readDeclaration(file)
            event.target.value = ""
          }}
        />
      </div>
      {editor.error && (
        <p role="alert" className="mb-3 break-words text-sm text-destructive">
          {editor.error}
        </p>
      )}
      {editor.status && (
        <p role="status" className="mb-3 text-sm text-muted-foreground">
          {editor.status}
        </p>
      )}
      {editor.draft ? (
        <ConnectorPublishForm key={editor.editorSession} editor={editor} />
      ) : (
        <>
          <div className="mb-4 flex gap-2">
            <Input
              aria-label="搜索我的连接器"
              placeholder="搜索我的连接器"
              value={catalog.search}
              onChange={(event) => catalog.setSearch(event.target.value)}
            />
            <Button variant="outline" onClick={catalog.refresh}>
              刷新
            </Button>
          </div>
          {catalog.loading && <p role="status">正在读取我的上传…</p>}
          {catalog.error ? (
            <div data-market-result-state role="alert">
              <p>{catalog.error}</p>
              <Button variant="outline" onClick={catalog.refresh}>
                重试
              </Button>
            </div>
          ) : (
            <div data-market-grid className={CAPABILITY_GRID_CLASS}>
              {catalog.items.map((item) => (
                <CapabilityCard
                  key={item.server_id}
                  kind="connector"
                  name={item.name}
                  summary={item.description}
                  publisher={item.owner}
                  category={item.category}
                  source="我的上传"
                  version={item.latest_version}
                  installed={false}
                  status={
                    item.listed
                      ? "已发布"
                      : item.published_version
                        ? "已下架 / 有草稿"
                        : "草稿"
                  }
                  onOpen={() => editor.open(item.server_id)}
                />
              ))}
            </div>
          )}
          {!catalog.loading && !catalog.error && !catalog.items.length && (
            <p
              data-market-result-state
              role="status"
              className="py-10 text-center text-sm text-muted-foreground"
            >
              还没有上传连接器，可新建或导入声明。
            </p>
          )}
          {!catalog.error && <MarketPagination {...catalog.pagination} />}
        </>
      )}
    </div>
  )
}
