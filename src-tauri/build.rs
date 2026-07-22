fn main() {
    #[cfg(feature = "tauri-runtime")]
    {
        ensure_sidecar_placeholders();
        tauri_build::build();
        link_windows_test_manifest();
    }
}

#[cfg(feature = "tauri-runtime")]
fn link_windows_test_manifest() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("windows") {
        return;
    }

    let dependency = "/MANIFESTDEPENDENCY:type='win32' \
        name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
        processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'";
    println!("cargo:rustc-link-arg={dependency}");
}

/// Tauri's bundler validates that every `bundle.externalBin` path resolves
/// to an existing file at build.rs time. The real `iyw-claw-mcp` sidecar is
/// produced by `pnpm tauri:prepare-sidecars` (invoked from
/// `beforeBuildCommand` / `beforeDevCommand` and the CI release matrix) —
/// but plain `cargo check --features tauri-runtime` doesn't go through that
/// path, so without a backstop every contributor would hit
/// `resource path ... doesn't exist` on first compile.
///
/// We write a zero-byte placeholder when the sidecar is missing so
/// `cargo check` / clippy / rust-analyzer succeed. Production paths
/// overwrite the placeholder with the real binary before Tauri bundles it:
///   * `pnpm tauri build`  → `beforeBuildCommand` → `prepare-sidecars.mjs`
///   * release.yml         → explicit "Stage iyw-claw-mcp sidecar" step
///   * `pnpm tauri dev`    → `beforeDevCommand` → `prepare-sidecars.mjs`
///
/// If you ever bypass those wrappers (e.g. invoking the Tauri CLI directly
/// without beforeBuildCommand) you'd ship the placeholder, so emit a
/// cargo:warning that surfaces in any compile log to make that loud.
#[cfg(feature = "tauri-runtime")]
fn ensure_sidecar_placeholders() {
    use std::path::PathBuf;

    let triple = std::env::var("TARGET").unwrap_or_default();
    if triple.is_empty() {
        return;
    }
    let ext = if triple.contains("windows") {
        ".exe"
    } else {
        ""
    };
    let dir = PathBuf::from("binaries");
    let versioned_mcp = format!("iyw-claw-mcp-{}", env!("CARGO_PKG_VERSION"));
    for name in ["iyw-claw-mcp", versioned_mcp.as_str(), "uv", "uvx"] {
        ensure_sidecar_placeholder(&dir, name, &triple, ext);
    }
}

#[cfg(feature = "tauri-runtime")]
fn ensure_sidecar_placeholder(dir: &std::path::Path, name: &str, triple: &str, ext: &str) {
    use std::fs;

    let path = dir.join(format!("{name}-{triple}{ext}"));

    println!("cargo:rerun-if-changed={}", path.display());

    let needs_placeholder = match fs::metadata(&path) {
        Ok(meta) => meta.len() == 0,
        Err(_) => true,
    };

    if needs_placeholder {
        if let Err(e) = fs::create_dir_all(dir) {
            panic!("failed to create {}: {e}", dir.display());
        }
        if let Err(e) = fs::write(&path, b"") {
            panic!(
                "failed to write sidecar placeholder {}: {e}",
                path.display()
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o755));
        }
        println!(
            "cargo:warning={name} sidecar missing at {}; wrote 0-byte placeholder. \
             Run `pnpm tauri:prepare-sidecars` before `tauri build` to ship a working binary.",
            path.display()
        );
    }
}
