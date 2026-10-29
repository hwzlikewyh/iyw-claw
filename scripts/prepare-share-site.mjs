import { cp, mkdir, stat } from "node:fs/promises"
import { resolve } from "node:path"
import { fileURLToPath } from "node:url"

const projectRoot = fileURLToPath(new URL("../", import.meta.url))
const exportRoot = resolve(projectRoot, "out")
const destination = resolve(
  process.argv[2] ?? resolve(projectRoot, "artifacts/release-share-site")
)

async function prepare() {
  await stat(resolve(exportRoot, "share.html"))
  await stat(resolve(exportRoot, "_next/static"))
  await mkdir(destination, { recursive: true })
  await cp(
    resolve(exportRoot, "share.html"),
    resolve(destination, "share.html")
  )
  await cp(
    resolve(exportRoot, "_next/static"),
    resolve(destination, "_next/static"),
    { recursive: true }
  )
  console.log(`分享网页已准备：${destination}`)
  console.log("将 share.html 和 _next/static 上传到 TOS 的相同对象路径。")
  console.log(
    "网页入口为 https://<TOS 自定义域名>/share.html；分享正文和成果仍由 API 校验权限。"
  )
}

prepare().catch((error) => {
  console.error("准备分享网页失败，请先完成 pnpm build。", error.message)
  process.exitCode = 1
})
