import type { NextConfig } from "next"
import createNextIntlPlugin from "next-intl/plugin"

const isProd = process.env.NODE_ENV === "production"
const internalHost = process.env.TAURI_DEV_HOST || "localhost"
const withNextIntl = createNextIntlPlugin({
  requestConfig: "./src/i18n/request.ts",
  experimental: {
    messages: {
      path: "./src/i18n/messages",
      format: "json",
      locales: ["en", "zh-CN"],
      precompile: true,
    },
  },
})

const nextConfig: NextConfig = {
  output: "export",
  // PDF.js 的 legacy viewer 仍含 Unicode sets 正则，须按 109 浏览器目标转译。
  transpilePackages: ["pdfjs-dist"],
  images: {
    unoptimized: true,
  },
  assetPrefix: isProd ? undefined : `http://${internalHost}:3000`,
}

export default withNextIntl(nextConfig)
