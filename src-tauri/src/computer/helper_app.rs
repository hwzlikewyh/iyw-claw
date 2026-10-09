//! The helper app as iyw-claw runs it on macOS: a copy outside iyw-claw's bundle.
//!
//! iyw-claw ships the helper as an app of its own inside its bundle
//! (`Contents/Helpers/iyw-computer-helper.app`), and for Accessibility that
//! is enough: macOS charges a process to the app it is the main executable
//! of. Screen Recording it charges to the *outermost* app around the
//! executable that the same team signed — iyw-claw — so a helper run from inside
//! iyw-claw's bundle would ask for Screen Recording in iyw-claw's name and record
//! the screen on iyw-claw's grant, which every agent's shell shares.
//!
//! So iyw-claw runs the helper from a copy of the shipped app in the helper's
//! data directory, where no app of iyw-claw's is around it. The copy is the
//! shipped app byte for byte — the same signature, so the same launch
//! requirement holds it and the grants are the same ones — and iyw-claw brings
//! it up to date before every launch: a copy that is missing, or that differs
//! from the shipped app in any way (an update, a damaged or altered copy), is
//! replaced whole by one made beside it, so a launch never finds half of one.
//! Extended attributes are left behind: the signature does not cover them,
//! and a quarantine flag on the copy would only have Gatekeeper assess it
//! again.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{self, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Make the copy of `shipped`, an app bundle, in `home` the same as it, and
/// say where the copy is.
pub fn install(shipped: &Path, home: &Path) -> io::Result<PathBuf> {
    // One at a time: the running helper and a permission request can both be
    // starting, and iyw-claw runs as a single instance, so this process is the
    // only one to do it.
    static INSTALLING: Mutex<()> = Mutex::new(());
    let _one = INSTALLING.lock().unwrap_or_else(|p| p.into_inner());

    let name = shipped
        .file_name()
        .ok_or_else(|| io::Error::other(format!("{} names no app", shipped.display())))?;
    let installed = home.join(name);
    if same_tree(shipped, &installed)? {
        return Ok(installed);
    }
    fs::create_dir_all(home)?;
    // Made beside it under a name nothing launches or lists as an app, then
    // put in its place.
    let mut prefix = OsString::from(".");
    prefix.push(name);
    prefix.push(".");
    sweep(home, &prefix);
    let mut staging = prefix;
    staging.push(format!("{}.{}.incoming", std::process::id(), unique()));
    let staging = home.join(staging);
    let made = copy_tree(shipped, &staging).and_then(|()| put_in_place(&staging, &installed));
    if made.is_err() {
        let _ = remove_entry(&staging);
    }
    made.map(|()| installed)
}

/// Throw away what earlier installs left in `home` under names starting with
/// `prefix` — a launch that died part-way, a copy that would not delete.
fn sweep(home: &Path, prefix: &OsString) {
    let Ok(entries) = fs::read_dir(home) else {
        return;
    };
    for entry in entries.flatten() {
        if entry
            .file_name()
            .as_encoded_bytes()
            .starts_with(prefix.as_encoded_bytes())
        {
            let _ = remove_entry(&entry.path());
        }
    }
}

/// Different on every call in this process.
fn unique() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Move `staging` to `installed`: exchanged with what is there in one step,
/// the old one then thrown away. A helper still running from the old one
/// keeps its image.
fn put_in_place(staging: &Path, installed: &Path) -> io::Result<()> {
    if fs::symlink_metadata(installed).is_err() {
        return fs::rename(staging, installed);
    }
    match exchange(staging, installed) {
        Ok(()) => {
            // `staging` now holds what was in place.
            let _ = remove_entry(staging);
            Ok(())
        }
        // A file system that cannot exchange two names, or something in the
        // copy's place it will not exchange with a directory.
        Err(_) => replace_by_renames(staging, installed),
    }
}

/// Put `staging` in `installed`'s place in two renames: the old one aside,
/// the new one in — and the old one back, should the new one not go in.
fn replace_by_renames(staging: &Path, installed: &Path) -> io::Result<()> {
    let aside = staging.with_extension("outgoing");
    fs::rename(installed, &aside)?;
    if let Err(e) = fs::rename(staging, installed) {
        let _ = fs::rename(&aside, installed);
        return Err(e);
    }
    let _ = remove_entry(&aside);
    Ok(())
}

/// Swap what two paths name, atomically (`renamex_np` with `RENAME_SWAP`).
fn exchange(a: &Path, b: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let a = CString::new(a.as_os_str().as_bytes())?;
    let b = CString::new(b.as_os_str().as_bytes())?;
    // SAFETY: two NUL-terminated paths that outlive the call.
    if unsafe { libc::renamex_np(a.as_ptr(), b.as_ptr(), libc::RENAME_SWAP) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Remove whatever `path` names — a directory and all in it, a file, a link
/// (not what it points at) — and nothing when it names nothing.
fn remove_entry(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
    }
}

/// Whether `copy` holds what `original` does: the same names, kinds,
/// permission bits, bytes and link targets all the way down. Trouble reading
/// `original` is an error; anything wrong with `copy` is only a difference.
fn same_tree(original: &Path, copy: &Path) -> io::Result<bool> {
    let meta = fs::symlink_metadata(original)?;
    let Ok(copied) = fs::symlink_metadata(copy) else {
        return Ok(false);
    };
    let kind = meta.file_type();
    if kind != copied.file_type() {
        return Ok(false);
    }
    if kind.is_symlink() {
        return Ok(fs::read_link(copy).ok() == Some(fs::read_link(original)?));
    }
    if mode(&meta) != mode(&copied) {
        return Ok(false);
    }
    if kind.is_dir() {
        let names = names_in(original)?;
        if names_in(copy).ok().as_ref() != Some(&names) {
            return Ok(false);
        }
        for name in &names {
            if !same_tree(&original.join(name), &copy.join(name))? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    if kind.is_file() {
        return Ok(meta.len() == copied.len() && same_bytes(original, copy)?);
    }
    Err(not_copied(original))
}

/// Whether two files hold the same bytes. Trouble reading `original` is an
/// error; trouble reading `copy` is a difference.
fn same_bytes(original: &Path, copy: &Path) -> io::Result<bool> {
    let mut original = File::open(original)?;
    let Ok(mut copy) = File::open(copy) else {
        return Ok(false);
    };
    let mut theirs = vec![0u8; 64 * 1024];
    let mut ours = vec![0u8; 64 * 1024];
    loop {
        let n = fill(&mut original, &mut theirs)?;
        let Ok(m) = fill(&mut copy, &mut ours) else {
            return Ok(false);
        };
        if theirs[..n] != ours[..m] {
            return Ok(false);
        }
        if n == 0 {
            return Ok(true);
        }
    }
}

/// Read into `buf` until it is full or the file ends; how much was read.
fn fill(file: &mut File, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match file.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

/// Copy `original` to `copy`, which does not exist yet: directories, files
/// (their bytes and permission bits, nothing else) and links, all the way
/// down.
fn copy_tree(original: &Path, copy: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(original)?;
    let kind = meta.file_type();
    if kind.is_symlink() {
        return std::os::unix::fs::symlink(fs::read_link(original)?, copy);
    }
    if kind.is_dir() {
        fs::create_dir(copy)?;
        for name in names_in(original)? {
            copy_tree(&original.join(&name), &copy.join(&name))?;
        }
    } else if kind.is_file() {
        let mut from = File::open(original)?;
        let mut to = OpenOptions::new().write(true).create_new(true).open(copy)?;
        io::copy(&mut from, &mut to)?;
        to.sync_all()?;
    } else {
        return Err(not_copied(original));
    }
    // Last, so a directory is still writable while it fills; and set outright,
    // so the umask leaves no mark.
    fs::set_permissions(copy, Permissions::from_mode(mode(&meta)))
}

/// A directory's entries, by name, in order.
fn names_in(dir: &Path) -> io::Result<Vec<OsString>> {
    let mut names = fs::read_dir(dir)?
        .map(|entry| entry.map(|e| e.file_name()))
        .collect::<io::Result<Vec<_>>>()?;
    names.sort();
    Ok(names)
}

/// The permission bits, without the file type or the set-id bits.
fn mode(meta: &fs::Metadata) -> u32 {
    meta.permissions().mode() & 0o777
}

fn not_copied(path: &Path) -> io::Error {
    io::Error::other(format!(
        "{} is not a file, a directory or a link",
        path.display()
    ))
}
