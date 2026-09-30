use std::io::{Error, ErrorKind, Read, Result as IoResult, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use portable_pty::cmdbuilder::CommandBuilder;
use portable_pty::{Child, ChildKiller, ExitStatus};
use iyw_codex_harness::JobObject;

const MIN_CONPTY_BUILD: u32 = 17_763;

#[repr(C)]
struct OsVersionInfoW {
    size: u32,
    major: u32,
    minor: u32,
    build: u32,
    platform: u32,
    service_pack: [u16; 128],
}

#[link(name = "ntdll")]
unsafe extern "system" {
    fn RtlGetVersion(version: *mut OsVersionInfoW) -> i32;
}

pub fn conpty_supported() -> bool {
    let mut version = OsVersionInfoW {
        size: std::mem::size_of::<OsVersionInfoW>() as u32,
        major: 0,
        minor: 0,
        build: 0,
        platform: 0,
        service_pack: [0; 128],
    };
    unsafe { RtlGetVersion(&mut version) == 0 && version.build >= MIN_CONPTY_BUILD }
}

struct PipeReader {
    receiver: Receiver<Vec<u8>>,
    current: Vec<u8>,
}

#[derive(Debug)]
struct PipeChild {
    child: std::process::Child,
    job: Arc<JobObject>,
}

#[derive(Debug)]
struct PipeChildKiller {
    job: Arc<JobObject>,
}

impl ChildKiller for PipeChildKiller {
    fn kill(&mut self) -> IoResult<()> {
        self.job.terminate()
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(Self {
            job: Arc::clone(&self.job),
        })
    }
}

impl ChildKiller for PipeChild {
    fn kill(&mut self) -> IoResult<()> {
        self.job.terminate()
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(PipeChildKiller {
            job: Arc::clone(&self.job),
        })
    }
}

impl Child for PipeChild {
    fn try_wait(&mut self) -> IoResult<Option<ExitStatus>> {
        self.child.try_wait().map(|status| status.map(Into::into))
    }

    fn wait(&mut self) -> IoResult<ExitStatus> {
        self.child.wait().map(Into::into)
    }

    fn process_id(&self) -> Option<u32> {
        Some(self.child.id())
    }

    fn as_raw_handle(&self) -> Option<std::os::windows::io::RawHandle> {
        Some(std::os::windows::io::AsRawHandle::as_raw_handle(&self.child))
    }
}

impl Read for PipeReader {
    fn read(&mut self, buffer: &mut [u8]) -> IoResult<usize> {
        if self.current.is_empty() {
            match self.receiver.recv() {
                Ok(chunk) => self.current = chunk,
                Err(_) => return Ok(0),
            }
        }
        let amount = buffer.len().min(self.current.len());
        buffer[..amount].copy_from_slice(&self.current[..amount]);
        self.current.drain(..amount);
        Ok(amount)
    }
}

fn forward_output(mut reader: impl Read + Send + 'static, sender: Sender<Vec<u8>>) {
    thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(amount) => {
                    if sender.send(buffer[..amount].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
}

pub fn spawn(
    command: CommandBuilder,
) -> IoResult<(
    Box<dyn Write + Send>,
    Box<dyn Read + Send>,
    Box<dyn portable_pty::Child + Send + Sync>,
)> {
    let argv = command.get_argv();
    let Some(program) = argv.first() else {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "missing Windows shell program",
        ));
    };
    let mut process = Command::new(program);
    process.args(argv.iter().skip(1));
    process.env_clear();
    process.envs(command.iter_full_env_as_str());
    if let Some(cwd) = command.get_cwd() {
        let cwd = PathBuf::from(cwd);
        if cwd.is_dir() {
            process.current_dir(cwd);
        }
    }
    process
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let job = JobObject::create().map_err(|error| {
        Error::new(
            ErrorKind::Other,
            format!("create Windows pipe terminal job: {error}"),
        )
    })?;
    let job = Arc::new(job);
    let mut child = job.spawn_std_process(&mut process)?;
    let writer = child
        .stdin
        .take()
        .ok_or_else(|| Error::other("Windows pipe terminal stdin unavailable"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::other("Windows pipe terminal stdout unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Error::other("Windows pipe terminal stderr unavailable"))?;
    let (sender, receiver) = mpsc::channel();
    forward_output(stdout, sender.clone());
    forward_output(stderr, sender);
    let reader = PipeReader {
        receiver,
        current: Vec::new(),
    };
    Ok((
        Box::new(writer),
        Box::new(reader),
        Box::new(PipeChild { child, job }),
    ))
}
