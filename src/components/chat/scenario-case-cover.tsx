import type { ScenarioCase } from "@/lib/scenario-cases"
import styles from "./scenario-cases.module.css"

const COVERS = {
  stationery: "/scenario-cases/stationery.png",
  survey: "/scenario-cases/survey.png",
  ceramics: "/scenario-cases/ceramics.png",
  quotation: "/scenario-cases/entry-yellow.png",
}

export function ScenarioCaseCover({
  item,
  onSelect,
}: {
  item: ScenarioCase
  onSelect: () => void
}) {
  return (
    <button
      type="button"
      className={styles.card}
      data-tone={item.coverTone}
      data-footer-tag={item.tagAtFooter || undefined}
      onClick={onSelect}
      aria-label={`查看案例：${item.title}`}
    >
      {/* 封面使用 Figma 原始素材，标题与标签由案例数据提供。 */}
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img src={COVERS[item.coverKey]} alt="" className={styles.cover} />
      <span className={styles.tint} aria-hidden="true" />
      <span className={styles.cardTitle}>{item.title}</span>
      {!item.tagAtFooter && <span className={styles.tag}>{item.tag}</span>}
      <span className={styles.cardFooter}>
        {item.tagAtFooter && <span className={styles.tag}>{item.tag}</span>}
        <span className={styles.viewButton}>立即查看</span>
      </span>
    </button>
  )
}

export function CaseEntryThumbnails() {
  return (
    <span className={styles.thumbnails} aria-hidden="true">
      {[
        ["survey.png", "cyan"],
        ["entry-yellow.png", "yellow"],
        ["stationery.png", "peach"],
      ].map(([file, tone]) => (
        <span key={file} className={styles.thumbnail} data-tone={tone}>
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img src={`/scenario-cases/${file}`} alt="" />
          <span className={styles.tint} />
        </span>
      ))}
    </span>
  )
}
