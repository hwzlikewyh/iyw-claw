import assert from "node:assert/strict"
import test from "node:test"

import { parseSha256, resolveUvRelease } from "./prepare-sidecars.mjs"

test("maps supported targets to pinned uv release archives", () => {
  assert.deepEqual(resolveUvRelease("x86_64-pc-windows-msvc"), {
    extension: "zip",
    url: "https://github.com/astral-sh/uv/releases/download/0.8.10/uv-x86_64-pc-windows-msvc.zip",
  })
  assert.deepEqual(resolveUvRelease("x86_64-unknown-linux-gnu"), {
    extension: "tar.gz",
    url: "https://github.com/astral-sh/uv/releases/download/0.8.10/uv-x86_64-unknown-linux-gnu.tar.gz",
  })
  assert.equal(
    resolveUvRelease("i686-pc-windows-msvc").url,
    "https://github.com/astral-sh/uv/releases/download/0.8.10/uv-i686-pc-windows-msvc.zip"
  )
})

test("parses official sha256 sidecar content", () => {
  const digest = "abcdef0123456789".repeat(4)
  assert.equal(
    parseSha256(`${digest.toUpperCase()}  uv-x86_64-pc-windows-msvc.zip\n`),
    digest
  )
})
