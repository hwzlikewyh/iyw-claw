// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Spawn per `spec`. Returns the child's handle; a suspended child stays
/// stopped until [`Child::resume`].
pub fn spawn(spec: &SpawnSpec<'_>) -> std::io::Result<Child> {
    let program = cstring(spec.program.as_os_str().as_bytes())?;
    let mut argv_owned = vec![program.clone()];
    for arg in spec.args {
        argv_owned.push(cstring(arg.as_bytes())?);
    }
    let mut argv: Vec<*mut libc::c_char> =
        argv_owned.iter().map(|s| s.as_ptr() as *mut _).collect();
    argv.push(std::ptr::null_mut());
    let env_owned: Vec<CString> = spec
        .env
        .iter()
        .map(|(k, v)| cstring(format!("{k}={v}").as_bytes()))
        .collect::<Result<_, _>>()?;
    let mut envp: Vec<*mut libc::c_char> = env_owned.iter().map(|s| s.as_ptr() as *mut _).collect();
    envp.push(std::ptr::null_mut());
    let dev_null = cstring(b"/dev/null")?;

    let mut attr: libc::posix_spawnattr_t = std::ptr::null_mut();
    let mut actions: libc::posix_spawn_file_actions_t = std::ptr::null_mut();
    // SAFETY: plain initialisers of the two out-parameters, destroyed below on
    // every path.
    unsafe {
        check(libc::posix_spawnattr_init(&mut attr))?;
        if let Err(e) = check(libc::posix_spawn_file_actions_init(&mut actions)) {
            libc::posix_spawnattr_destroy(&mut attr);
            return Err(e);
        }
    }
    let result = (|| -> std::io::Result<libc::pid_t> {
        let mut flags = libc::POSIX_SPAWN_CLOEXEC_DEFAULT;
        if spec.suspended {
            flags |= libc::POSIX_SPAWN_START_SUSPENDED;
        }
        // SAFETY: `attr` / `actions` were initialised above; every pointer
        // passed below outlives the `posix_spawn` call that reads it.
        unsafe {
            check(libc::posix_spawnattr_setflags(
                &mut attr,
                flags as libc::c_short,
            ))?;
            if let Some(requirement) = spec.launch_requirement {
                crate::computer::launch_req::apply(&mut attr, requirement)?;
            }
            if spec.disclaim {
                let disclaim = disclaim_fn().ok_or_else(|| {
                    std::io::Error::other(
                        "this macOS cannot launch a process as its own TCC principal \
                         (responsibility_spawnattrs_setdisclaim is missing)",
                    )
                })?;
                check(disclaim(&mut attr, 1))?;
            }
            for (slot, fd) in spec.stdio.iter().enumerate() {
                let slot = slot as c_int;
                match *fd {
                    ChildFd::Inherit(fd) => {
                        check(libc::posix_spawn_file_actions_adddup2(
                            &mut actions,
                            fd,
                            slot,
                        ))?;
                    }
                    ChildFd::Null => {
                        let mode = if slot == 0 {
                            libc::O_RDONLY
                        } else {
                            libc::O_WRONLY
                        };
                        check(libc::posix_spawn_file_actions_addopen(
                            &mut actions,
                            slot,
                            dev_null.as_ptr(),
                            mode,
                            0,
                        ))?;
                    }
                }
            }
            let mut pid: libc::pid_t = 0;
            check(libc::posix_spawn(
                &mut pid,
                program.as_ptr(),
                &actions,
                &attr,
                argv.as_mut_ptr(),
                envp.as_mut_ptr(),
            ))?;
            Ok(pid)
        }
    })();
    // SAFETY: both were initialised above and are destroyed exactly once.
    unsafe {
        libc::posix_spawn_file_actions_destroy(&mut actions);
        libc::posix_spawnattr_destroy(&mut attr);
    }
    let pid = result?;
    Child::new(pid as u32)
}
