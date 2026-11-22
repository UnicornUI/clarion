//! Single-instance activation socket. A second launch focuses the existing
//! window (or selects a pane) instead of opening another settings sheet.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

const MODE: u32 = 0o600;

#[derive(Clone, Debug)]
pub enum Signal {
    Focus,
    Pane(String),
}

pub fn socket_path() -> PathBuf {
    std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .filter(|path| path.starts_with('/'))
        .map(PathBuf::from)
        .map(|runtime| runtime.join("clarion-settings.sock"))
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("clarion-settings-{}.sock", unsafe {
                libc::getuid()
            }))
        })
}

fn same_user(stream: &UnixStream) -> io::Result<()> {
    let mut peer: libc::ucred = unsafe { std::mem::zeroed() };
    let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let status = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut peer as *mut _ as *mut libc::c_void,
            &mut size,
        )
    };
    if status != 0 {
        return Err(io::Error::last_os_error());
    }
    if peer.uid != unsafe { libc::geteuid() } {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(())
}

fn read_signal(stream: &mut UnixStream) -> io::Result<Signal> {
    same_user(stream)?;
    stream.set_read_timeout(Some(Duration::from_millis(700)))?;
    let mut line = String::new();
    let mut byte = [0u8; 1];
    while line.len() < 1024 {
        match stream.read(&mut byte)? {
            0 => break,
            1 if byte[0] == b'\n' => break,
            1 => line.push(byte[0] as char),
            _ => unreachable!(),
        }
    }
    let line = line.trim();
    if let Some(pane) = line.strip_prefix("PANE:") {
        Ok(Signal::Pane(pane.to_owned()))
    } else {
        Ok(Signal::Focus)
    }
}

pub fn signal_existing(path: &Path, pane: Option<&str>) -> io::Result<bool> {
    let mut stream = match UnixStream::connect(path) {
        Ok(stream) => stream,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ) =>
        {
            return Ok(false);
        }
        Err(error) => return Err(error),
    };
    stream.set_write_timeout(Some(Duration::from_millis(700)))?;
    let command = pane
        .map(|pane| format!("PANE:{pane}\n"))
        .unwrap_or_else(|| "FOCUS\n".into());
    stream.write_all(command.as_bytes())?;
    Ok(true)
}

pub struct Instance {
    path: PathBuf,
    _lock: File,
}

impl Instance {
    pub fn bind(path: PathBuf, tx: std::sync::mpsc::Sender<Signal>) -> io::Result<Self> {
        let lock_path = path.with_extension("lock");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(MODE)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
            .open(&lock_path)?;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(io::Error::last_os_error());
        }
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if !metadata.file_type().is_socket() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "settings socket is not a socket",
                ));
            }
            match UnixStream::connect(&path) {
                Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                    fs::remove_file(&path)?
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::AddrInUse,
                        "settings is already running",
                    ))
                }
            }
        }
        let listener = UnixListener::bind(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(MODE))?;
        std::thread::Builder::new()
            .name("clarion-settings-activation".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let mut stream = stream;
                    if let Ok(signal) = read_signal(&mut stream) {
                        let _ = tx.send(signal);
                    }
                }
            })
            .ok();
        Ok(Self {
            path,
            _lock: lock,
        })
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
