use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

#[cfg(unix)]
use std::{
    sync::{
        atomic::Ordering,
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
};

use thiserror::Error;

#[cfg(unix)]
const WATCH_MASK: u32 = libc::IN_CLOSE_WRITE
    | libc::IN_MOVED_TO
    | libc::IN_CREATE
    | libc::IN_ATTRIB
    | libc::IN_DELETE
    | libc::IN_MOVED_FROM
    | libc::IN_DELETE_SELF
    | libc::IN_MOVE_SELF
    | libc::IN_IGNORED
    | libc::IN_Q_OVERFLOW;

pub struct ConfigWatcher {
    dirty: Arc<AtomicBool>,
    healthy: Arc<AtomicBool>,
    #[cfg(unix)]
    stop_fd: Option<std::os::unix::io::RawFd>,
    #[cfg(unix)]
    thread: Option<JoinHandle<()>>,
}

impl ConfigWatcher {
    #[cfg(unix)]
    pub fn start(paths: &[PathBuf]) -> Result<(Self, Receiver<()>), ConfigWatchError> {
        let paths = validate_paths(paths)?;
        let mut stop_pipe = [0; 2];
        // SAFETY: pipe2 fills two valid descriptors.
        if unsafe { libc::pipe2(stop_pipe.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } < 0 {
            return Err(ConfigWatchError::Initialize);
        }
        let dirty = Arc::new(AtomicBool::new(false));
        let healthy = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::channel();
        let dirty_thread = Arc::clone(&dirty);
        let healthy_thread = Arc::clone(&healthy);
        let stop_read = stop_pipe[0];
        let stop_write = stop_pipe[1];
        let thread = thread::Builder::new()
            .name("nethop-config-watch".to_owned())
            .spawn(move || {
                let _ = sender.send(());
                let mut retry_seconds = 1_000;
                loop {
                    match open_watches(&paths) {
                        Ok((inotify, watches)) => {
                            set_health(&healthy_thread, true, &sender);
                            watch_loop(
                                inotify,
                                stop_read,
                                watches,
                                &dirty_thread,
                                &healthy_thread,
                                &sender,
                            );
                            // SAFETY: inotify is owned by this thread.
                            unsafe { libc::close(inotify) };
                            break;
                        }
                        Err(_) => {
                            set_health(&healthy_thread, false, &sender);
                            if wait_for_stop(stop_read, retry_seconds) {
                                break;
                            }
                            retry_seconds = (retry_seconds * 2).min(60_000);
                        }
                    }
                }
                // SAFETY: stop_read is owned by this thread after spawn.
                unsafe { libc::close(stop_read) };
            })
            .map_err(|_| ConfigWatchError::Initialize)?;
        Ok((
            Self {
                dirty,
                healthy,
                stop_fd: Some(stop_write),
                thread: Some(thread),
            },
            receiver,
        ))
    }

    #[cfg(not(unix))]
    pub fn start(
        _paths: &[PathBuf],
    ) -> Result<(Self, std::sync::mpsc::Receiver<()>), ConfigWatchError> {
        Err(ConfigWatchError::Unsupported)
    }

    pub fn dirty(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.dirty)
    }

    pub fn healthy(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.healthy)
    }
}

#[cfg(unix)]
fn open_watches(paths: &[PathBuf]) -> Result<(libc::c_int, Vec<WatchTarget>), ConfigWatchError> {
    // SAFETY: flags are valid and no pointers are passed.
    let inotify = unsafe { libc::inotify_init1(libc::IN_CLOEXEC | libc::IN_NONBLOCK) };
    if inotify < 0 {
        return Err(ConfigWatchError::Initialize);
    }
    let mut watches = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = match std::ffi::CString::new(path.as_os_str().as_encoded_bytes()) {
            Ok(bytes) => bytes,
            Err(_) => {
                // SAFETY: inotify was opened above and is still owned here.
                unsafe { libc::close(inotify) };
                return Err(ConfigWatchError::InvalidPath);
            }
        };
        // SAFETY: CString is NUL terminated and inotify owns the watch state.
        let descriptor = unsafe { libc::inotify_add_watch(inotify, bytes.as_ptr(), WATCH_MASK) };
        if descriptor < 0 {
            // SAFETY: inotify was opened above and is still owned here.
            unsafe { libc::close(inotify) };
            return Err(ConfigWatchError::Initialize);
        }
        watches.push(WatchTarget {
            descriptor: Some(descriptor),
            path: path.clone(),
        });
    }
    Ok((inotify, watches))
}

#[cfg(unix)]
fn wait_for_stop(stop_read: libc::c_int, timeout_ms: libc::c_int) -> bool {
    let mut descriptor = libc::pollfd {
        fd: stop_read,
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: descriptor points to one valid pollfd value.
    unsafe { libc::poll(&mut descriptor, 1, timeout_ms) > 0 }
}

#[cfg(unix)]
fn set_health(healthy: &AtomicBool, value: bool, sender: &mpsc::Sender<()>) {
    if healthy.swap(value, Ordering::AcqRel) != value {
        let _ = sender.send(());
    }
}

#[cfg(unix)]
fn validate_paths(paths: &[PathBuf]) -> Result<Vec<PathBuf>, ConfigWatchError> {
    if paths.is_empty() {
        return Err(ConfigWatchError::InvalidPath);
    }
    let mut unique = Vec::with_capacity(paths.len());
    for path in paths {
        if !path.is_absolute() || !path.is_dir() || path.file_name().is_none() {
            return Err(ConfigWatchError::InvalidPath);
        }
        let canonical = path
            .canonicalize()
            .map_err(|_| ConfigWatchError::InvalidPath)?;
        if !unique.contains(&canonical) {
            unique.push(canonical);
        }
    }
    Ok(unique)
}

#[cfg(unix)]
fn watch_loop(
    inotify: libc::c_int,
    stop_read: libc::c_int,
    mut watches: Vec<WatchTarget>,
    dirty: &AtomicBool,
    healthy: &AtomicBool,
    sender: &mpsc::Sender<()>,
) {
    let mut buffer = vec![0_u8; 16 * 1024];
    loop {
        let mut poll_fds = [
            libc::pollfd {
                fd: inotify,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: stop_read,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let timeout = if watches.iter().any(|watch| watch.descriptor.is_none()) {
            1_000
        } else {
            -1
        };
        // SAFETY: pollfd array is valid for its fixed length.
        let result = unsafe { libc::poll(poll_fds.as_mut_ptr(), poll_fds.len() as _, timeout) };
        if result == 0 {
            reinstall_missing_watches(inotify, &mut watches);
            set_health(
                healthy,
                watches.iter().all(|watch| watch.descriptor.is_some()),
                sender,
            );
            continue;
        }
        if result < 0 {
            continue;
        }
        if poll_fds[1].revents != 0 {
            break;
        }
        if poll_fds[0].revents == 0 {
            continue;
        }
        // SAFETY: buffer is writable and inotify fd is owned by this thread.
        let count = unsafe {
            libc::read(
                inotify,
                buffer.as_mut_ptr().cast::<libc::c_void>(),
                buffer.len(),
            )
        };
        if count <= 0 {
            continue;
        }
        let mut offset = 0_usize;
        while offset < count as usize {
            // SAFETY: kernel guarantees each complete event fits in the returned buffer;
            // read_unaligned avoids assuming the byte buffer's alignment.
            let event = unsafe {
                buffer
                    .as_ptr()
                    .add(offset)
                    .cast::<libc::inotify_event>()
                    .read_unaligned()
            };
            let name_start = offset + std::mem::size_of::<libc::inotify_event>();
            let name_end = name_start.saturating_add(event.len as usize);
            let name = if event.len == 0 || name_end > count as usize {
                None
            } else {
                let bytes = &buffer[name_start..name_end];
                let length = bytes
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(bytes.len());
                std::str::from_utf8(&bytes[..length]).ok()
            };
            let relevant = event_relevant(event.mask, name);
            if relevant && !dirty.swap(true, Ordering::AcqRel) {
                let _ = sender.send(());
            }
            if event.mask & libc::IN_IGNORED != 0 {
                if let Some(watch) = watches
                    .iter_mut()
                    .find(|watch| watch.descriptor == Some(event.wd))
                {
                    watch.descriptor = None;
                }
            }
            let step = std::mem::size_of::<libc::inotify_event>() + event.len as usize;
            if step == 0 {
                break;
            }
            offset = offset.saturating_add(step);
        }
        reinstall_missing_watches(inotify, &mut watches);
        set_health(
            healthy,
            watches.iter().all(|watch| watch.descriptor.is_some()),
            sender,
        );
    }
}

#[cfg(unix)]
#[derive(Debug)]
struct WatchTarget {
    descriptor: Option<libc::c_int>,
    path: PathBuf,
}

#[cfg(unix)]
fn reinstall_missing_watches(inotify: libc::c_int, watches: &mut [WatchTarget]) {
    for watch in watches
        .iter_mut()
        .filter(|watch| watch.descriptor.is_none())
    {
        let Ok(bytes) = std::ffi::CString::new(watch.path.as_os_str().as_encoded_bytes()) else {
            continue;
        };
        // SAFETY: path CString and inotify descriptor are valid.
        let descriptor = unsafe { libc::inotify_add_watch(inotify, bytes.as_ptr(), WATCH_MASK) };
        if descriptor >= 0 {
            watch.descriptor = Some(descriptor);
        }
    }
}

#[cfg(unix)]
fn event_relevant(mask: u32, name: Option<&str>) -> bool {
    mask & libc::IN_Q_OVERFLOW != 0
        || mask & (libc::IN_DELETE_SELF | libc::IN_MOVE_SELF | libc::IN_IGNORED) != 0
        || name == Some("nethop.toml")
}

impl Drop for ConfigWatcher {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(stop_fd) = self.stop_fd.take() {
            // SAFETY: stop_fd is owned by this handle and a one-byte wake is enough.
            unsafe {
                let _ = libc::write(stop_fd, [1_u8].as_ptr().cast(), 1);
                libc::close(stop_fd);
            }
        }
        #[cfg(unix)]
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigWatchError {
    #[error("configuration watch path is invalid")]
    InvalidPath,
    #[error("configuration watcher could not be initialized")]
    Initialize,
    #[error("configuration watcher is unsupported on this platform")]
    Unsupported,
}

#[cfg(all(test, unix))]
mod tests {
    use std::{fs, sync::atomic::Ordering, time::Duration};

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn watcher_coalesces_config_events_and_shutdown_wakes_poll() {
        let directory = tempdir().unwrap();
        let (watcher, receiver) = ConfigWatcher::start(&[directory.path().to_owned()]).unwrap();
        fs::write(directory.path().join("ignored.txt"), "ignored").unwrap();
        assert!(receiver.recv_timeout(Duration::from_millis(50)).is_err());

        fs::write(directory.path().join("nethop.toml"), "first").unwrap();
        receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        fs::write(directory.path().join("nethop.toml"), "second").unwrap();
        assert!(watcher.dirty().load(Ordering::Acquire));
        drop(watcher);
    }

    #[test]
    fn overflow_and_watch_invalidation_are_always_relevant() {
        assert!(event_relevant(libc::IN_Q_OVERFLOW, None));
        assert!(event_relevant(libc::IN_IGNORED, None));
        assert!(event_relevant(libc::IN_MOVED_TO, Some("nethop.toml")));
        assert!(!event_relevant(libc::IN_MOVED_TO, Some("other.toml")));
    }
}
