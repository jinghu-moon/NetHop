use nethop_subscription::{
    CandidateStatus, CapabilityMatrix, ParserIpcRequest, ParserIpcResponse, ParserLimits,
    ReceivedAt, SourceInput, convert_stable_sources,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FakeRootManager {
    Magisk,
    KernelSu,
}

#[derive(Debug, Default)]
pub struct FakeGenerationStore {
    current_digest: Option<String>,
    source_cache_digest: Option<String>,
}

impl FakeGenerationStore {
    pub fn current_digest(&self) -> Option<&str> {
        self.current_digest.as_deref()
    }

    pub fn source_cache_digest(&self) -> Option<&str> {
        self.source_cache_digest.as_deref()
    }

    pub fn commit_ready(&mut self, response: &ParserIpcResponse) -> bool {
        let CandidateStatus::Ready {
            candidate_digest, ..
        } = response.candidate()
        else {
            return false;
        };
        self.current_digest = Some(candidate_digest.clone());
        self.source_cache_digest = Some(candidate_digest.clone());
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FakePeer {
    pub uid: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FakeHostError {
    PermissionDenied,
    TimedOut,
    InvalidRequest,
    InvalidResponse,
}

pub struct FakeModuleParserHost {
    manager: FakeRootManager,
    timeout_millis: u64,
    limits: ParserLimits,
}

impl FakeModuleParserHost {
    pub fn new(manager: FakeRootManager, timeout_millis: u64) -> Self {
        Self {
            manager,
            timeout_millis,
            limits: ParserLimits::default(),
        }
    }

    pub fn manager(&self) -> FakeRootManager {
        self.manager
    }

    pub fn handle(
        &self,
        peer: FakePeer,
        elapsed_millis: u64,
        frame: &[u8],
    ) -> Result<String, FakeHostError> {
        if peer.uid != 0 {
            return Err(FakeHostError::PermissionDenied);
        }
        if elapsed_millis > self.timeout_millis {
            return Err(FakeHostError::TimedOut);
        }
        let request = ParserIpcRequest::from_json(frame, &self.limits)
            .map_err(|_| FakeHostError::InvalidRequest)?;
        let payload = request
            .to_import_payload(
                ReceivedAt {
                    wall_clock_unix_ms: 1,
                    monotonic_nanos: 1,
                },
                &self.limits,
            )
            .map_err(|_| FakeHostError::InvalidRequest)?;
        let conversion = convert_stable_sources(
            vec![SourceInput {
                source_id: request.source_id().clone(),
                format_hint: request.expected_format(),
                bytes: payload.bytes().to_vec(),
            }],
            &self.limits,
            &CapabilityMatrix::default(),
        );
        ParserIpcResponse::from_conversion(request.request_id().clone(), &conversion, &self.limits)
            .and_then(|response| response.to_json(&self.limits))
            .map_err(|_| FakeHostError::InvalidResponse)
    }
}
