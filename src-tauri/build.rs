mod build_computer;

fn main() {
    build_computer::export_identity();
    export_target_triple();
    place_computer_helper_for_development();

    #[cfg(feature = "tauri-runtime")]
    {
        ensure_sidecar_placeholders();
        let windows = tauri_build::WindowsAttributes::new()
            .app_manifest(include_str!("windows/application.manifest"));
        tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
            .expect("Tauri build failed");
        link_windows_test_manifest();
    }
}

fn export_target_triple() {
    let target = std::env::var("TARGET").expect("Cargo TARGET is unavailable");
    println!("cargo:rustc-env=IYW_CLAW_TARGET_TRIPLE={target}");
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
/// to an existing file at build.rs time. The real application sidecars are
/// produced by `pnpm tauri:prepare-sidecars` (invoked from
/// `beforeBuildCommand` / `beforeDevCommand` and the CI release matrix) —
/// but plain `cargo check --features tauri-runtime` doesn't go through that
/// path, so without a backstop every contributor would hit
/// `resource path ... doesn't exist` on first compile.
///
/// We write a zero-byte placeholder when the environment helper is missing so
/// `cargo check` / clippy / rust-analyzer succeed. Production paths
/// overwrite the placeholder with the real binary before Tauri bundles it:
///   * `pnpm tauri build`  → `beforeBuildCommand` → `prepare-sidecars.mjs`
///   * release.yml         → explicit sidecar staging step
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
    if matches!(
        triple.as_str(),
        "x86_64-pc-windows-msvc"
            | "x86_64-win7-windows-msvc"
            | "x86_64-apple-darwin"
            | "aarch64-apple-darwin"
            | "x86_64-unknown-linux-gnu"
            | "aarch64-unknown-linux-gnu"
    ) {
        ensure_sidecar_placeholder(&dir, "iyw-environment", &triple, ext);
        ensure_sidecar_placeholder(&dir, "iyw-computer-helper", &triple, ext);
    }
}

fn place_computer_helper_for_development() {
    if std::env::var("PROFILE").as_deref() != Ok("debug") { return; }
    let triple = std::env::var("TARGET").unwrap_or_default();
    let ext = if triple.contains("windows") { ".exe" } else { "" };
    let staged = std::path::PathBuf::from(format!("binaries/iyw-computer-helper-{triple}{ext}"));
    println!("cargo:rerun-if-changed={}", staged.display());
    if !std::fs::metadata(&staged).is_ok_and(|meta| meta.len() > 0) { return; }
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR missing"));
    if let Some(profile) = out.ancestors().nth(3) {
        std::fs::copy(staged, profile.join(format!("iyw-computer-helper{ext}"))).expect("stage development computer helper");
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
