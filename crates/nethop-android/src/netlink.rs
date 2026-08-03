use std::{collections::BTreeSet, time::Duration};

use thiserror::Error;

use crate::IpFamily;

const NETLINK_HEADER_BYTES: usize = 16;
const MAX_DATAGRAM_BYTES: usize = 64 * 1024;
const MAX_MESSAGES_PER_DATAGRAM: usize = 1_024;
const DEFAULT_QUIET_PERIOD: Duration = Duration::from_millis(250);
const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(2);

const NLMSG_NOOP: u16 = 1;
const NLMSG_ERROR: u16 = 2;
const NLMSG_DONE: u16 = 3;
const RTM_NEWLINK: u16 = 16;
const RTM_DELLINK: u16 = 17;
const RTM_NEWADDR: u16 = 20;
const RTM_DELADDR: u16 = 21;
const RTM_NEWROUTE: u16 = 24;
const RTM_DELROUTE: u16 = 25;
const AF_INET: u8 = 2;
const AF_INET6: u8 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NetworkAction {
    Upsert,
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NetworkChange {
    Link,
    Address(IpFamily),
    Route(IpFamily),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NetworkEvent {
    action: NetworkAction,
    change: NetworkChange,
}

impl NetworkEvent {
    pub const fn new(action: NetworkAction, change: NetworkChange) -> Self {
        Self { action, change }
    }

    pub const fn action(self) -> NetworkAction {
        self.action
    }

    pub const fn change(self) -> NetworkChange {
        self.change
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkEventBatch {
    events: BTreeSet<NetworkEvent>,
    event_count: u32,
    first_observed_at: Duration,
    last_observed_at: Duration,
}

impl NetworkEventBatch {
    pub fn events(&self) -> &BTreeSet<NetworkEvent> {
        &self.events
    }

    pub const fn event_count(&self) -> u32 {
        self.event_count
    }

    pub const fn first_observed_at(&self) -> Duration {
        self.first_observed_at
    }

    pub const fn last_observed_at(&self) -> Duration {
        self.last_observed_at
    }
}

#[derive(Debug, Error)]
pub enum NetlinkError {
    #[error("rtnetlink datagram exceeds the bounded buffer")]
    DatagramTooLarge,
    #[error("rtnetlink datagram is malformed")]
    MalformedDatagram,
    #[error("rtnetlink datagram contains too many messages")]
    TooManyMessages,
    #[error("rtnetlink reported a kernel error")]
    KernelError,
    #[error("monotonic event time moved backwards")]
    ClockRegressed,
    #[error("invalid debounce limits")]
    InvalidDebounceLimits,
    #[error("rtnetlink I/O failed")]
    Io(#[source] std::io::Error),
}

pub trait NetlinkEventSource {
    fn receive_datagram(&mut self, buffer: &mut [u8]) -> Result<usize, NetlinkError>;
}

#[derive(Debug)]
pub struct NetlinkEventReader<S> {
    source: S,
    buffer: Vec<u8>,
}

impl<S> NetlinkEventReader<S>
where
    S: NetlinkEventSource,
{
    pub fn new(source: S) -> Self {
        Self {
            source,
            buffer: vec![0; MAX_DATAGRAM_BYTES],
        }
    }

    pub fn receive(&mut self) -> Result<Vec<NetworkEvent>, NetlinkError> {
        let received = self.source.receive_datagram(&mut self.buffer)?;
        if received > self.buffer.len() {
            return Err(NetlinkError::DatagramTooLarge);
        }
        parse_datagram(&self.buffer[..received])
    }

    pub fn into_inner(self) -> S {
        self.source
    }
}

#[derive(Debug, Clone)]
pub struct NetlinkDebouncer {
    quiet_period: Duration,
    max_delay: Duration,
    pending: Option<NetworkEventBatch>,
    last_clock: Option<Duration>,
}

impl Default for NetlinkDebouncer {
    fn default() -> Self {
        Self::new(DEFAULT_QUIET_PERIOD, DEFAULT_MAX_DELAY)
            .expect("default debounce limits are valid")
    }
}

impl NetlinkDebouncer {
    pub fn new(quiet_period: Duration, max_delay: Duration) -> Result<Self, NetlinkError> {
        if quiet_period.is_zero() || max_delay < quiet_period {
            return Err(NetlinkError::InvalidDebounceLimits);
        }
        Ok(Self {
            quiet_period,
            max_delay,
            pending: None,
            last_clock: None,
        })
    }

    pub const fn quiet_period(&self) -> Duration {
        self.quiet_period
    }

    pub const fn max_delay(&self) -> Duration {
        self.max_delay
    }

    pub fn deadline(&self) -> Option<Duration> {
        self.pending.as_ref().map(|batch| {
            batch
                .last_observed_at
                .saturating_add(self.quiet_period)
                .min(batch.first_observed_at.saturating_add(self.max_delay))
        })
    }

    pub fn observe(
        &mut self,
        now: Duration,
        event: NetworkEvent,
    ) -> Result<Option<NetworkEventBatch>, NetlinkError> {
        self.validate_clock(now)?;
        let ready = if self.deadline().is_some_and(|deadline| now >= deadline) {
            self.pending.take()
        } else {
            None
        };
        match &mut self.pending {
            Some(batch) => {
                batch.events.insert(event);
                batch.event_count = batch.event_count.saturating_add(1);
                batch.last_observed_at = now;
            }
            None => {
                self.pending = Some(NetworkEventBatch {
                    events: BTreeSet::from([event]),
                    event_count: 1,
                    first_observed_at: now,
                    last_observed_at: now,
                });
            }
        }
        Ok(ready)
    }

    pub fn take_ready(&mut self, now: Duration) -> Result<Option<NetworkEventBatch>, NetlinkError> {
        self.validate_clock(now)?;
        if self.deadline().is_some_and(|deadline| now >= deadline) {
            Ok(self.pending.take())
        } else {
            Ok(None)
        }
    }

    fn validate_clock(&mut self, now: Duration) -> Result<(), NetlinkError> {
        if self.last_clock.is_some_and(|last| now < last) {
            return Err(NetlinkError::ClockRegressed);
        }
        self.last_clock = Some(now);
        Ok(())
    }
}

fn parse_datagram(bytes: &[u8]) -> Result<Vec<NetworkEvent>, NetlinkError> {
    if bytes.len() > MAX_DATAGRAM_BYTES {
        return Err(NetlinkError::DatagramTooLarge);
    }
    let mut events = Vec::new();
    let mut offset = 0usize;
    let mut messages = 0usize;
    while offset < bytes.len() {
        if bytes.len() - offset < NETLINK_HEADER_BYTES {
            return Err(NetlinkError::MalformedDatagram);
        }
        messages += 1;
        if messages > MAX_MESSAGES_PER_DATAGRAM {
            return Err(NetlinkError::TooManyMessages);
        }
        let length = u32::from_ne_bytes(
            bytes[offset..offset + 4]
                .try_into()
                .map_err(|_| NetlinkError::MalformedDatagram)?,
        ) as usize;
        let message_type = u16::from_ne_bytes(
            bytes[offset + 4..offset + 6]
                .try_into()
                .map_err(|_| NetlinkError::MalformedDatagram)?,
        );
        if length < NETLINK_HEADER_BYTES || length > bytes.len() - offset {
            return Err(NetlinkError::MalformedDatagram);
        }
        let payload = &bytes[offset + NETLINK_HEADER_BYTES..offset + length];
        if let Some(event) = parse_message(message_type, payload)? {
            events.push(event);
        }
        let aligned = length
            .checked_add(3)
            .map(|value| value & !3)
            .ok_or(NetlinkError::MalformedDatagram)?;
        offset = offset
            .checked_add(aligned)
            .ok_or(NetlinkError::MalformedDatagram)?;
        if offset > bytes.len() {
            return Err(NetlinkError::MalformedDatagram);
        }
    }
    Ok(events)
}

fn parse_message(message_type: u16, payload: &[u8]) -> Result<Option<NetworkEvent>, NetlinkError> {
    let event = match message_type {
        NLMSG_NOOP | NLMSG_DONE => return Ok(None),
        NLMSG_ERROR => return Err(NetlinkError::KernelError),
        RTM_NEWLINK => NetworkEvent::new(NetworkAction::Upsert, NetworkChange::Link),
        RTM_DELLINK => NetworkEvent::new(NetworkAction::Remove, NetworkChange::Link),
        RTM_NEWADDR => NetworkEvent::new(
            NetworkAction::Upsert,
            NetworkChange::Address(parse_family(payload)?),
        ),
        RTM_DELADDR => NetworkEvent::new(
            NetworkAction::Remove,
            NetworkChange::Address(parse_family(payload)?),
        ),
        RTM_NEWROUTE => NetworkEvent::new(
            NetworkAction::Upsert,
            NetworkChange::Route(parse_family(payload)?),
        ),
        RTM_DELROUTE => NetworkEvent::new(
            NetworkAction::Remove,
            NetworkChange::Route(parse_family(payload)?),
        ),
        _ => return Ok(None),
    };
    Ok(Some(event))
}

fn parse_family(payload: &[u8]) -> Result<IpFamily, NetlinkError> {
    match payload.first().copied() {
        Some(AF_INET) => Ok(IpFamily::Ipv4),
        Some(AF_INET6) => Ok(IpFamily::Ipv6),
        _ => Err(NetlinkError::MalformedDatagram),
    }
}

#[cfg(any(target_os = "android", target_os = "linux"))]
#[derive(Debug)]
pub struct NetlinkRouteSocket {
    descriptor: std::os::fd::RawFd,
}

#[cfg(any(target_os = "android", target_os = "linux"))]
impl NetlinkRouteSocket {
    pub fn open() -> Result<Self, NetlinkError> {
        use std::{io, mem};

        const GROUPS: u32 = 0x1 | 0x10 | 0x40 | 0x100 | 0x400;
        // SAFETY: socket arguments are fixed Linux ABI constants and ownership is
        // transferred immediately into the returned RAII wrapper.
        let descriptor = unsafe {
            libc::socket(
                libc::AF_NETLINK,
                libc::SOCK_RAW | libc::SOCK_CLOEXEC,
                libc::NETLINK_ROUTE,
            )
        };
        if descriptor < 0 {
            return Err(NetlinkError::Io(io::Error::last_os_error()));
        }
        // SAFETY: zero is a valid initialization for sockaddr_nl before the
        // public family, pid and multicast group fields are assigned.
        let mut address: libc::sockaddr_nl = unsafe { mem::zeroed() };
        address.nl_family = libc::AF_NETLINK as libc::sa_family_t;
        address.nl_pid = 0;
        address.nl_groups = GROUPS;
        // SAFETY: address points to a fully initialized sockaddr_nl and the
        // supplied length matches that concrete structure.
        let result = unsafe {
            libc::bind(
                descriptor,
                (&raw const address).cast::<libc::sockaddr>(),
                mem::size_of::<libc::sockaddr_nl>() as libc::socklen_t,
            )
        };
        if result < 0 {
            let error = io::Error::last_os_error();
            // SAFETY: descriptor is owned here and has not been closed.
            unsafe { libc::close(descriptor) };
            return Err(NetlinkError::Io(error));
        }
        Ok(Self { descriptor })
    }
}

#[cfg(any(target_os = "android", target_os = "linux"))]
impl NetlinkEventSource for NetlinkRouteSocket {
    fn receive_datagram(&mut self, buffer: &mut [u8]) -> Result<usize, NetlinkError> {
        use std::io;

        // SAFETY: buffer is valid for writes of buffer.len() bytes for the
        // duration of recv, and the owned descriptor remains open.
        let received = unsafe {
            libc::recv(
                self.descriptor,
                buffer.as_mut_ptr().cast::<libc::c_void>(),
                buffer.len(),
                0,
            )
        };
        if received < 0 {
            return Err(NetlinkError::Io(io::Error::last_os_error()));
        }
        Ok(received as usize)
    }
}

#[cfg(any(target_os = "android", target_os = "linux"))]
impl Drop for NetlinkRouteSocket {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns the descriptor and Drop runs once.
        unsafe { libc::close(self.descriptor) };
    }
}
