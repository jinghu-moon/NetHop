use std::time::Duration;

use nethopd::{ControlServerError, ControlServerLimits, PeerCredentials, RootPeerAuthorizer};

#[test]
fn root_authorizer_is_fail_closed() {
    let authorizer = RootPeerAuthorizer;
    assert!(
        authorizer
            .authorize(PeerCredentials::new(Some(1), 0, 0))
            .is_ok()
    );
    assert_eq!(
        authorizer
            .authorize(PeerCredentials::new(Some(22), 10_000, 10_000))
            .unwrap_err(),
        ControlServerError::AuthorizationDenied
    );
}

#[test]
fn server_timeout_is_bounded() {
    assert_eq!(
        ControlServerLimits::new(Duration::ZERO).unwrap_err(),
        ControlServerError::InvalidLimits
    );
    assert_eq!(
        ControlServerLimits::new(Duration::from_secs(31)).unwrap_err(),
        ControlServerError::InvalidLimits
    );
}

#[cfg(unix)]
mod unix {
    use std::{
        fs,
        os::unix::{fs::PermissionsExt, net::UnixStream},
        thread,
    };

    use nethop_protocol::{
        ControlMethod, ControlRequest, ControlResponse, FrameCodec, RequestId, WireFrame,
    };
    use nethopd::{ControlRequestHandler, UnixControlServer};
    use serde_json::json;
    use tempfile::tempdir;

    use super::*;

    struct Handler;

    impl ControlRequestHandler for Handler {
        fn handle(&mut self, request: ControlRequest) -> ControlResponse {
            ControlResponse::success(
                request.request_id().clone(),
                Some(7),
                json!({"state":"running"}),
            )
        }
    }

    #[test]
    fn server_enforces_socket_mode_root_peer_and_one_request_frame() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("nethopd.sock");
        let server = UnixControlServer::bind(&path, ControlServerLimits::default()).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        let worker = thread::spawn(move || {
            let mut handler = Handler;
            server.serve_once(&mut handler).unwrap()
        });
        let mut client = UnixStream::connect(&path).unwrap();
        FrameCodec::write_to(
            &mut client,
            &WireFrame::Request(ControlRequest::new(
                RequestId::new("req-uds").unwrap(),
                ControlMethod::StatusGet,
            )),
        )
        .unwrap();
        let response = FrameCodec::read_from(&mut client).unwrap();
        assert!(matches!(response, WireFrame::Response(response) if response.ok()));
        assert_eq!(worker.join().unwrap().uid(), 0);
        assert!(!path.exists());
    }

    #[test]
    fn server_refuses_occupied_or_relative_socket_paths() {
        let directory = tempdir().unwrap();
        let occupied = directory.path().join("occupied.sock");
        fs::write(&occupied, b"foreign").unwrap();
        assert_eq!(
            UnixControlServer::bind(&occupied, ControlServerLimits::default()).unwrap_err(),
            ControlServerError::SocketPathOccupied
        );
        assert_eq!(
            UnixControlServer::bind("relative.sock", ControlServerLimits::default()).unwrap_err(),
            ControlServerError::InvalidSocketPath
        );
    }

    #[test]
    fn nonblocking_server_reports_idle_without_hiding_real_requests() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("nethopd.sock");
        let server = UnixControlServer::bind(&path, ControlServerLimits::default()).unwrap();
        server.set_nonblocking(true).unwrap();
        let mut handler = Handler;
        assert_eq!(server.try_serve_once(&mut handler).unwrap(), None);

        let mut client = UnixStream::connect(&path).unwrap();
        FrameCodec::write_to(
            &mut client,
            &WireFrame::Request(ControlRequest::new(
                RequestId::new("req-nonblocking").unwrap(),
                ControlMethod::StatusGet,
            )),
        )
        .unwrap();
        assert!(server.try_serve_once(&mut handler).unwrap().is_some());
        assert!(matches!(
            FrameCodec::read_from(&mut client).unwrap(),
            WireFrame::Response(response) if response.ok()
        ));
    }
}
