use std::time::Duration;

use nethop_protocol::{ControlRequest, ControlResponse};
use thiserror::Error;

// Synchronous source mutations include bounded download and generation
// activation work before the final response is written.
const DEFAULT_IO_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_IO_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerCredentials {
    pid: Option<u32>,
    uid: u32,
    gid: u32,
}

impl PeerCredentials {
    pub const fn new(pid: Option<u32>, uid: u32, gid: u32) -> Self {
        Self { pid, uid, gid }
    }

    pub const fn pid(self) -> Option<u32> {
        self.pid
    }

    pub const fn uid(self) -> u32 {
        self.uid
    }

    pub const fn gid(self) -> u32 {
        self.gid
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct RootPeerAuthorizer;

impl RootPeerAuthorizer {
    pub fn authorize(self, peer: PeerCredentials) -> Result<(), ControlServerError> {
        (peer.uid == 0)
            .then_some(())
            .ok_or(ControlServerError::AuthorizationDenied)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlServerLimits {
    io_timeout: Duration,
}

impl ControlServerLimits {
    pub fn new(io_timeout: Duration) -> Result<Self, ControlServerError> {
        if io_timeout.is_zero() || io_timeout > MAX_IO_TIMEOUT {
            return Err(ControlServerError::InvalidLimits);
        }
        Ok(Self { io_timeout })
    }

    pub const fn io_timeout(self) -> Duration {
        self.io_timeout
    }
}

impl Default for ControlServerLimits {
    fn default() -> Self {
        Self {
            io_timeout: DEFAULT_IO_TIMEOUT,
        }
    }
}

pub trait ControlRequestHandler {
    fn handle(&mut self, request: ControlRequest) -> ControlResponse;

    fn subscribe_events(&mut self, _request: &ControlRequest) -> Option<crate::EventSubscription> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ControlServerError {
    #[error("control server limits are invalid")]
    InvalidLimits,
    #[error("control socket path is not an absolute path in a real directory")]
    InvalidSocketPath,
    #[error("control socket path is already occupied")]
    SocketPathOccupied,
    #[error("control socket could not be bound or configured")]
    BindFailed,
    #[error("control connection could not be accepted")]
    AcceptFailed,
    #[error("control peer credentials could not be read")]
    PeerCredentialFailed,
    #[error("control peer is not root")]
    AuthorizationDenied,
    #[error("control request frame is invalid")]
    InvalidRequest,
    #[error("control response frame could not be written")]
    ResponseFailed,
}

#[cfg(unix)]
mod unix {
    use std::{
        fs,
        os::{
            fd::AsRawFd,
            unix::{
                fs::{FileTypeExt, MetadataExt, PermissionsExt},
                net::{UnixListener, UnixStream},
            },
        },
        path::{Path, PathBuf},
    };

    use nethop_protocol::{FrameCodec, WireFrame};

    use super::*;

    #[derive(Debug)]
    pub struct UnixControlServer {
        listener: UnixListener,
        socket_path: PathBuf,
        socket_device: u64,
        socket_inode: u64,
        limits: ControlServerLimits,
        authorizer: RootPeerAuthorizer,
    }

    impl UnixControlServer {
        pub fn bind(
            socket_path: impl Into<PathBuf>,
            limits: ControlServerLimits,
        ) -> Result<Self, ControlServerError> {
            let socket_path = socket_path.into();
            validate_socket_path(&socket_path)?;
            match fs::symlink_metadata(&socket_path) {
                Ok(metadata) => reclaim_stale_socket(&socket_path, &metadata)?,
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    return Err(ControlServerError::BindFailed);
                }
                Err(_) => {}
            }
            let listener =
                UnixListener::bind(&socket_path).map_err(|_| ControlServerError::BindFailed)?;
            if fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600)).is_err() {
                drop(listener);
                let _ = remove_owned_socket(&socket_path, None);
                return Err(ControlServerError::BindFailed);
            }
            let metadata =
                fs::symlink_metadata(&socket_path).map_err(|_| ControlServerError::BindFailed)?;
            if !metadata.file_type().is_socket() || metadata.permissions().mode() & 0o777 != 0o600 {
                drop(listener);
                let _ = remove_owned_socket(&socket_path, None);
                return Err(ControlServerError::BindFailed);
            }
            Ok(Self {
                listener,
                socket_path,
                socket_device: metadata.dev(),
                socket_inode: metadata.ino(),
                limits,
                authorizer: RootPeerAuthorizer,
            })
        }

        pub fn socket_path(&self) -> &Path {
            &self.socket_path
        }

        pub fn set_nonblocking(&self, nonblocking: bool) -> Result<(), ControlServerError> {
            self.listener
                .set_nonblocking(nonblocking)
                .map_err(|_| ControlServerError::AcceptFailed)
        }

        pub fn serve_once(
            &self,
            handler: &mut impl ControlRequestHandler,
        ) -> Result<PeerCredentials, ControlServerError> {
            let (stream, _) = self
                .listener
                .accept()
                .map_err(|_| ControlServerError::AcceptFailed)?;
            self.serve_stream(stream, handler)
        }

        pub fn try_serve_once(
            &self,
            handler: &mut impl ControlRequestHandler,
        ) -> Result<Option<PeerCredentials>, ControlServerError> {
            match self.listener.accept() {
                Ok((stream, _)) => self.serve_stream(stream, handler).map(Some),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
                Err(_) => Err(ControlServerError::AcceptFailed),
            }
        }

        fn serve_stream(
            &self,
            mut stream: UnixStream,
            handler: &mut impl ControlRequestHandler,
        ) -> Result<PeerCredentials, ControlServerError> {
            stream
                .set_read_timeout(Some(self.limits.io_timeout))
                .map_err(|_| ControlServerError::AcceptFailed)?;
            stream
                .set_write_timeout(Some(self.limits.io_timeout))
                .map_err(|_| ControlServerError::AcceptFailed)?;
            let peer = peer_credentials(&stream)?;
            self.authorizer.authorize(peer)?;
            let request = match FrameCodec::read_from(&mut stream) {
                Ok(WireFrame::Request(request)) => request,
                _ => return Err(ControlServerError::InvalidRequest),
            };
            if request.method() == nethop_protocol::ControlMethod::EventsSubscribe
                && let Some(mut subscription) = handler.subscribe_events(&request)
            {
                std::thread::Builder::new()
                    .name("nethop-events".into())
                    .spawn(move || {
                        while let Ok(frame) = subscription.next_frame() {
                            if FrameCodec::write_to(&mut stream, &WireFrame::Stream(frame)).is_err()
                            {
                                break;
                            }
                        }
                    })
                    .map_err(|_| ControlServerError::ResponseFailed)?;
                return Ok(peer);
            }
            let response = WireFrame::Response(handler.handle(request));
            FrameCodec::write_to(&mut stream, &response)
                .map_err(|_| ControlServerError::ResponseFailed)?;
            Ok(peer)
        }
    }

    impl Drop for UnixControlServer {
        fn drop(&mut self) {
            let _ = remove_owned_socket(
                &self.socket_path,
                Some((self.socket_device, self.socket_inode)),
            );
        }
    }

    fn validate_socket_path(path: &Path) -> Result<(), ControlServerError> {
        if !path.is_absolute() || path.file_name().is_none() {
            return Err(ControlServerError::InvalidSocketPath);
        }
        let parent = path.parent().ok_or(ControlServerError::InvalidSocketPath)?;
        let metadata =
            fs::symlink_metadata(parent).map_err(|_| ControlServerError::InvalidSocketPath)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(ControlServerError::InvalidSocketPath);
        }
        let canonical = parent
            .canonicalize()
            .map_err(|_| ControlServerError::InvalidSocketPath)?;
        if canonical != parent {
            return Err(ControlServerError::InvalidSocketPath);
        }
        Ok(())
    }

    fn peer_credentials(stream: &UnixStream) -> Result<PeerCredentials, ControlServerError> {
        let mut credentials = libc::ucred {
            pid: 0,
            uid: 0,
            gid: 0,
        };
        let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        // SAFETY: credentials and length point to writable objects of the
        // expected SO_PEERCRED ABI sizes for this Unix stream descriptor.
        let result = unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&raw mut credentials).cast::<libc::c_void>(),
                &raw mut length,
            )
        };
        if result != 0 || length as usize != std::mem::size_of::<libc::ucred>() {
            return Err(ControlServerError::PeerCredentialFailed);
        }
        let pid = u32::try_from(credentials.pid).ok().filter(|pid| *pid != 0);
        Ok(PeerCredentials::new(pid, credentials.uid, credentials.gid))
    }

    fn reclaim_stale_socket(
        path: &Path,
        metadata: &fs::Metadata,
    ) -> Result<(), ControlServerError> {
        if !metadata.file_type().is_socket() || socket_inode_is_live(metadata.ino())? {
            return Err(ControlServerError::SocketPathOccupied);
        }
        remove_owned_socket(path, Some((metadata.dev(), metadata.ino())))
            .map_err(|_| ControlServerError::BindFailed)?;
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            _ => Err(ControlServerError::SocketPathOccupied),
        }
    }

    fn socket_inode_is_live(inode: u64) -> Result<bool, ControlServerError> {
        let sockets = fs::read_to_string("/proc/net/unix")
            .map_err(|_| ControlServerError::SocketPathOccupied)?;
        Ok(sockets.lines().skip(1).any(|line| {
            line.split_ascii_whitespace()
                .nth(6)
                .and_then(|value| value.parse::<u64>().ok())
                == Some(inode)
        }))
    }

    fn remove_owned_socket(
        path: &Path,
        identity: Option<(u64, u64)>,
    ) -> Result<(), std::io::Error> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        if !metadata.file_type().is_socket()
            || identity
                .is_some_and(|(device, inode)| metadata.dev() != device || metadata.ino() != inode)
        {
            return Ok(());
        }
        fs::remove_file(path)
    }
}

#[cfg(unix)]
pub use unix::UnixControlServer;
