use std::{collections::VecDeque, time::Duration};

use nethop_android::{
    IpFamily, NetlinkDebouncer, NetlinkError, NetlinkEventReader, NetlinkEventSource,
    NetworkAction, NetworkChange, NetworkEvent,
};

#[derive(Debug)]
struct FakeSource {
    datagrams: VecDeque<Vec<u8>>,
}

impl NetlinkEventSource for FakeSource {
    fn receive_datagram(&mut self, buffer: &mut [u8]) -> Result<usize, NetlinkError> {
        let datagram = self.datagrams.pop_front().unwrap();
        buffer[..datagram.len()].copy_from_slice(&datagram);
        Ok(datagram.len())
    }
}

fn message(message_type: u16, family: Option<u8>) -> Vec<u8> {
    let length = 16 + usize::from(family.is_some());
    let aligned = (length + 3) & !3;
    let mut bytes = vec![0; aligned];
    bytes[..4].copy_from_slice(&(length as u32).to_ne_bytes());
    bytes[4..6].copy_from_slice(&message_type.to_ne_bytes());
    if let Some(family) = family {
        bytes[16] = family;
    }
    bytes
}

#[test]
fn reader_maps_only_link_address_and_route_events() {
    let mut datagram = message(16, None);
    datagram.extend(message(21, Some(10)));
    datagram.extend(message(24, Some(2)));
    datagram.extend(message(99, None));
    let source = FakeSource {
        datagrams: VecDeque::from([datagram]),
    };

    let events = NetlinkEventReader::new(source).receive().unwrap();
    assert_eq!(
        events,
        [
            NetworkEvent::new(NetworkAction::Upsert, NetworkChange::Link),
            NetworkEvent::new(
                NetworkAction::Remove,
                NetworkChange::Address(IpFamily::Ipv6),
            ),
            NetworkEvent::new(NetworkAction::Upsert, NetworkChange::Route(IpFamily::Ipv4),),
        ]
    );
}

#[test]
fn reader_rejects_truncated_messages_and_kernel_errors() {
    let truncated = FakeSource {
        datagrams: VecDeque::from([vec![20, 0, 0, 0, 16, 0, 0, 0]]),
    };
    assert!(matches!(
        NetlinkEventReader::new(truncated).receive(),
        Err(NetlinkError::MalformedDatagram)
    ));

    let kernel_error = FakeSource {
        datagrams: VecDeque::from([message(2, None)]),
    };
    assert!(matches!(
        NetlinkEventReader::new(kernel_error).receive(),
        Err(NetlinkError::KernelError)
    ));
}

#[test]
fn debounce_waits_for_quiet_period_and_deduplicates_change_kinds() {
    let event = NetworkEvent::new(
        NetworkAction::Upsert,
        NetworkChange::Address(IpFamily::Ipv6),
    );
    let mut debounce = NetlinkDebouncer::default();
    assert_eq!(debounce.quiet_period(), Duration::from_millis(250));
    assert_eq!(debounce.max_delay(), Duration::from_secs(2));
    assert!(debounce.observe(Duration::ZERO, event).unwrap().is_none());
    assert!(
        debounce
            .observe(Duration::from_millis(100), event)
            .unwrap()
            .is_none()
    );
    assert!(
        debounce
            .take_ready(Duration::from_millis(349))
            .unwrap()
            .is_none()
    );

    let batch = debounce
        .take_ready(Duration::from_millis(350))
        .unwrap()
        .unwrap();
    assert_eq!(batch.event_count(), 2);
    assert_eq!(batch.events().len(), 1);
    assert_eq!(batch.first_observed_at(), Duration::ZERO);
    assert_eq!(batch.last_observed_at(), Duration::from_millis(100));
}

#[test]
fn continuous_churn_is_flushed_at_two_seconds() {
    let mut debounce = NetlinkDebouncer::default();
    for millis in (0..=1_800).step_by(200) {
        assert!(
            debounce
                .observe(
                    Duration::from_millis(millis),
                    NetworkEvent::new(NetworkAction::Upsert, NetworkChange::Link),
                )
                .unwrap()
                .is_none()
        );
    }
    assert_eq!(debounce.deadline(), Some(Duration::from_secs(2)));
    let batch = debounce
        .take_ready(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    assert_eq!(batch.event_count(), 10);
}

#[test]
fn event_arriving_after_deadline_emits_old_batch_and_starts_new_one() {
    let mut debounce = NetlinkDebouncer::default();
    debounce
        .observe(
            Duration::ZERO,
            NetworkEvent::new(NetworkAction::Upsert, NetworkChange::Link),
        )
        .unwrap();
    let old = debounce
        .observe(
            Duration::from_millis(300),
            NetworkEvent::new(NetworkAction::Remove, NetworkChange::Route(IpFamily::Ipv4)),
        )
        .unwrap()
        .unwrap();
    assert_eq!(old.event_count(), 1);
    assert_eq!(debounce.deadline(), Some(Duration::from_millis(550)));
}
