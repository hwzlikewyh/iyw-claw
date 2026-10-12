import styles from "./scenario-cases.module.css"

export function CaseLoadError({
  onRetry,
  phase = "asset",
}: {
  onRetry: () => void
  phase?: "detail" | "asset"
}) {
  return (
    <div className={styles.loadState} role="alert">
      <p>
        {phase === "detail"
          ? "案例详情暂时无法读取，请重试。"
          : "案例素材暂时无法读取，请重试。"}
      </p>
      <button type="button" className={styles.viewButton} onClick={onRetry}>
        重新加载
      </button>
    </div>
  )
}
