use std::{
    collections::{BTreeSet, VecDeque},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use nethop_protocol::{EventKind, LogChannel, RequestId, StreamFrame};
use serde_json::{Value, json};
use thiserror::Error;

const DEFAULT_CAPACITY: usize = 128;
const MAX_CAPACITY: usize = 1_024;
const MAX_SUBSCRIBERS: usize = 4;
const MAX_LOG_LINE_BYTES: usize = 16 * 1024;
const MAX_LOG_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_LOG_HISTORY_ITEMS: usize = 128;
const MAX_LOG_HISTORY_FILES: usize = 32;
const MAX_LOG_HISTORY_BYTES: usize = 256 * 1024;
const SECONDS_PER_DAY: u64 = 24 * 60 * 60;

#[derive(Debug, Clone)]
pub struct EventHub {
    shared: Arc<Shared>,
}

#[derive(Debug)]
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    capacity: usize,
    log: Mutex<Option<FileEventLog>>,
}

#[derive(Debug)]
struct State {
    next_sequence: u64,
    subscribers: usize,
    traffic_subscribers: usize,
    snapshot: Value,
    events: VecDeque<EventRecord>,
    latest_traffic: Option<EventRecord>,
}

#[derive(Debug, Clone)]
struct EventRecord {
    sequence: u64,
    kind: EventKind,
    payload: Value,
}

impl EventHub {
    pub fn new(snapshot: Value, capacity: usize) -> Result<Self, EventError> {
        if capacity == 0 || capacity > MAX_CAPACITY || !snapshot.is_object() {
            return Err(EventError::InvalidPolicy);
        }
        Ok(Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    next_sequence: 1,
                    subscribers: 0,
                    traffic_subscribers: 0,
                    snapshot,
                    events: VecDeque::with_capacity(capacity),
                    latest_traffic: None,
                }),
                changed: Condvar::new(),
                capacity,
                log: Mutex::new(None),
            }),
        })
    }

    pub fn install_file_log(&self, directory: impl Into<PathBuf>) -> Result<(), EventError> {
        let log = FileEventLog::new(directory.into())?;
        let mut installed = self
            .shared
            .log
            .lock()
            .map_err(|_| EventError::Unavailable)?;
        *installed = Some(log);
        Ok(())
    }

    pub fn structured_log_history(
        &self,
        channel: Option<LogChannel>,
        limit: u8,
    ) -> Result<Vec<Value>, EventError> {
        if limit == 0 || usize::from(limit) > MAX_LOG_HISTORY_ITEMS {
            return Err(EventError::InvalidPolicy);
        }
        let mut installed = self
            .shared
            .log
            .lock()
            .map_err(|_| EventError::Unavailable)?;
        installed
            .as_mut()
            .ok_or(EventError::Unavailable)?
            .history(channel, usize::from(limit))
    }

    pub fn clear_structured_logs(&self) -> Result<usize, EventError> {
        let mut installed = self
            .shared
            .log
            .lock()
            .map_err(|_| EventError::Unavailable)?;
        installed.as_mut().ok_or(EventError::Unavailable)?.clear()
    }

    pub fn publish(&self, kind: EventKind, payload: Value) {
        if !payload.is_object() {
            return;
        }
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let sequence = state.next_sequence;
        state.next_sequence = state.next_sequence.saturating_add(1).max(1);
        if kind == EventKind::Traffic {
            state.latest_traffic = Some(EventRecord {
                sequence,
                kind,
                payload,
            });
            self.shared.changed.notify_all();
            return;
        }
        if state.events.len() == self.shared.capacity {
            state.events.pop_front();
        }
        state.events.push_back(EventRecord {
            sequence,
            kind,
            payload: payload.clone(),
        });
        self.shared.changed.notify_all();
        drop(state);
        if let Ok(mut log) = self.shared.log.lock()
            && let Some(log) = log.as_mut()
        {
            let _ = log.write(sequence, kind, &payload);
        }
    }

    pub fn traffic_subscribers(&self) -> usize {
        self.shared
            .state
            .lock()
            .map(|state| state.traffic_subscribers)
            .unwrap_or_default()
    }

    pub fn replace_snapshot(&self, snapshot: Value) {
        if !snapshot.is_object() {
            return;
        }
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.snapshot = snapshot;
    }

    pub fn subscribe(
        &self,
        request_id: RequestId,
        kinds: &[EventKind],
    ) -> Result<EventSubscription, EventError> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| EventError::Unavailable)?;
        if state.subscribers >= MAX_SUBSCRIBERS {
            return Err(EventError::Busy);
        }
        state.subscribers += 1;
        let traffic_enabled = kinds.is_empty() || kinds.contains(&EventKind::Traffic);
        if traffic_enabled {
            state.traffic_subscribers += 1;
        }
        let cursor = state.next_sequence;
        let snapshot = state.snapshot.clone();
        Ok(EventSubscription {
            shared: Arc::clone(&self.shared),
            request_id,
            kinds: kinds.iter().copied().collect(),
            cursor,
            traffic_cursor: 0,
            traffic_enabled,
            output_sequence: 1,
            initial_snapshot: Some(snapshot),
        })
    }
}

#[derive(Debug)]
struct FileEventLog {
    directory: PathBuf,
    day: Option<u64>,
    file: Option<File>,
    bytes: u64,
}

impl FileEventLog {
    fn new(directory: PathBuf) -> Result<Self, EventError> {
        if !directory.is_absolute() {
            return Err(EventError::InvalidLogDirectory);
        }
        let metadata =
            fs::symlink_metadata(&directory).map_err(|_| EventError::InvalidLogDirectory)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(EventError::InvalidLogDirectory);
        }
        Ok(Self {
            directory,
            day: None,
            file: None,
            bytes: 0,
        })
    }

    fn write(&mut self, sequence: u64, kind: EventKind, payload: &Value) -> Result<(), EventError> {
        let day = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| EventError::Unavailable)?
            .as_secs()
            / SECONDS_PER_DAY;
        if self.day != Some(day) {
            let path = self.directory.join(format!("events-{day:010}.log"));
            let (file, bytes) = open_private_append(&path)?;
            self.file = Some(file);
            self.day = Some(day);
            self.bytes = bytes;
        }
        if self.bytes >= MAX_LOG_FILE_BYTES {
            return Ok(());
        }
        let mut line = serde_json::to_vec(&json!({
            "seq": sequence,
            "kind": kind,
            "payload": payload,
        }))
        .map_err(|_| EventError::Unavailable)?;
        line.push(b'\n');
        if line.len() > MAX_LOG_LINE_BYTES
            || self.bytes.saturating_add(line.len() as u64) > MAX_LOG_FILE_BYTES
        {
            return Ok(());
        }
        let file = self.file.as_mut().ok_or(EventError::Unavailable)?;
        file.write_all(&line).map_err(|_| EventError::Unavailable)?;
        self.bytes += line.len() as u64;
        Ok(())
    }

    fn history(
        &mut self,
        channel: Option<LogChannel>,
        limit: usize,
    ) -> Result<Vec<Value>, EventError> {
        if let Some(file) = self.file.as_mut() {
            file.flush().map_err(|_| EventError::Unavailable)?;
        }
        let mut paths = controlled_log_paths(&self.directory)?;
        paths.sort();
        let mut history = Vec::with_capacity(limit);
        let mut retained_bytes = 0_usize;
        for path in paths.into_iter().rev().take(MAX_LOG_HISTORY_FILES) {
            let file = open_private_read(&path)?;
            let metadata = file.metadata().map_err(|_| EventError::Unavailable)?;
            if metadata.len() > MAX_LOG_FILE_BYTES {
                continue;
            }
            let mut bytes = Vec::with_capacity(metadata.len() as usize);
            file.take(MAX_LOG_FILE_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| EventError::Unavailable)?;
            if bytes.len() as u64 > MAX_LOG_FILE_BYTES {
                continue;
            }
            for line in bytes.split(|byte| *byte == b'\n').rev() {
                if line.is_empty() || line.len() > MAX_LOG_LINE_BYTES {
                    continue;
                }
                if retained_bytes.saturating_add(line.len()) > MAX_LOG_HISTORY_BYTES {
                    return Ok(history);
                }
                let Ok(mut value) = serde_json::from_slice::<Value>(line) else {
                    continue;
                };
                if !value.is_object() {
                    continue;
                }
                let Some(entry_channel) = value
                    .get("kind")
                    .and_then(|kind| serde_json::from_value::<EventKind>(kind.clone()).ok())
                    .and_then(log_channel_for_event)
                else {
                    continue;
                };
                if channel.is_some_and(|requested| requested != entry_channel) {
                    continue;
                }
                redact_sensitive(&mut value);
                let raw = serde_json::to_string(&value).unwrap_or_default();
                if let Some(object) = value.as_object_mut() {
                    object.insert("channel".into(), json!(entry_channel));
                    object.insert("raw".into(), Value::String(raw));
                }
                retained_bytes += line.len();
                history.push(value);
                if history.len() == limit {
                    return Ok(history);
                }
            }
        }
        Ok(history)
    }

    fn clear(&mut self) -> Result<usize, EventError> {
        self.file = None;
        self.day = None;
        self.bytes = 0;
        let paths = controlled_log_paths(&self.directory)?;
        let mut removed = 0_usize;
        for path in paths {
            fs::remove_file(path).map_err(|_| EventError::Unavailable)?;
            removed += 1;
        }
        Ok(removed)
    }
}

const fn log_channel_for_event(kind: EventKind) -> Option<LogChannel> {
    match kind {
        EventKind::Subscription => Some(LogChannel::Subscription),
        EventKind::Runtime | EventKind::Generation => Some(LogChannel::Core),
        EventKind::Config | EventKind::Network => Some(LogChannel::Service),
        EventKind::Traffic => None,
    }
}

fn controlled_log_paths(directory: &Path) -> Result<Vec<PathBuf>, EventError> {
    let entries = fs::read_dir(directory).map_err(|_| EventError::Unavailable)?;
    let mut paths = Vec::new();
    for entry in entries.take(1_024) {
        let entry = entry.map_err(|_| EventError::Unavailable)?;
        let path = entry.path();
        if path.parent() != Some(directory)
            || path.extension().and_then(|value| value.to_str()) != Some("log")
        {
            continue;
        }
        let metadata = fs::symlink_metadata(&path).map_err(|_| EventError::Unavailable)?;
        if metadata.is_file() && !metadata.file_type().is_symlink() {
            paths.push(path);
        }
    }
    Ok(paths)
}

fn open_private_read(path: &Path) -> Result<File, EventError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    options.open(path).map_err(|_| EventError::Unavailable)
}

fn redact_sensitive(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if matches!(
                    key.to_ascii_lowercase().as_str(),
                    "url" | "password" | "secret" | "token" | "uuid" | "authorization"
                ) {
                    *value = Value::String("[REDACTED]".into());
                } else {
                    redact_sensitive(value);
                }
            }
        }
        Value::Array(values) => values.iter_mut().for_each(redact_sensitive),
        _ => {}
    }
}

fn open_private_append(path: &Path) -> Result<(File, u64), EventError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err(EventError::Unavailable),
    };
    if metadata
        .as_ref()
        .is_some_and(|metadata| !metadata.is_file() || metadata.file_type().is_symlink())
    {
        return Err(EventError::InvalidLogDirectory);
    }
    let mut options = OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = options.open(path).map_err(|_| EventError::Unavailable)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| EventError::Unavailable)?;
    }
    let bytes = file.metadata().map_err(|_| EventError::Unavailable)?.len();
    Ok((file, bytes))
}

impl Default for EventHub {
    fn default() -> Self {
        Self::new(json!({"kind":"snapshot","state":"init"}), DEFAULT_CAPACITY)
            .expect("default event policy is valid")
    }
}

pub struct EventSubscription {
    shared: Arc<Shared>,
    request_id: RequestId,
    kinds: BTreeSet<EventKind>,
    cursor: u64,
    traffic_cursor: u64,
    traffic_enabled: bool,
    output_sequence: u64,
    initial_snapshot: Option<Value>,
}

impl EventSubscription {
    pub fn next_frame(&mut self) -> Result<StreamFrame, EventError> {
        if let Some(snapshot) = self.initial_snapshot.take() {
            return Ok(self.item(snapshot));
        }
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| EventError::Unavailable)?;
        loop {
            if let Some(oldest) = state.events.front().map(|event| event.sequence)
                && self.cursor < oldest
            {
                self.cursor = state.next_sequence;
                self.initial_snapshot = Some(state.snapshot.clone());
                drop(state);
                return Ok(self.item(json!({"kind":"resync_required"})));
            }
            let normal = state
                .events
                .iter()
                .find(|event| event.sequence >= self.cursor && self.accepts(event.kind))
                .cloned();
            let traffic = self
                .traffic_enabled
                .then(|| state.latest_traffic.as_ref())
                .flatten()
                .filter(|event| event.sequence > self.traffic_cursor)
                .cloned();
            if normal.as_ref().is_some_and(|normal| {
                traffic
                    .as_ref()
                    .is_none_or(|traffic| normal.sequence < traffic.sequence)
            }) {
                let event = normal.expect("normal event was checked");
                self.cursor = event.sequence.saturating_add(1);
                drop(state);
                return Ok(self.item(event.payload));
            }
            if let Some(event) = traffic {
                self.traffic_cursor = event.sequence;
                drop(state);
                return Ok(self.item(event.payload));
            }
            self.cursor = self.cursor.max(state.next_sequence);
            state = self
                .shared
                .changed
                .wait(state)
                .map_err(|_| EventError::Unavailable)?;
        }
    }

    fn accepts(&self, kind: EventKind) -> bool {
        self.kinds.is_empty() || self.kinds.contains(&kind)
    }

    fn item(&mut self, payload: Value) -> StreamFrame {
        let sequence = self.output_sequence;
        self.output_sequence = self.output_sequence.saturating_add(1).max(1);
        StreamFrame::item(self.request_id.clone(), sequence, payload)
    }
}

impl Drop for EventSubscription {
    fn drop(&mut self) {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.subscribers = state.subscribers.saturating_sub(1);
        if self.traffic_enabled {
            state.traffic_subscribers = state.traffic_subscribers.saturating_sub(1);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum EventError {
    #[error("event stream policy is invalid")]
    InvalidPolicy,
    #[error("event log directory is invalid")]
    InvalidLogDirectory,
    #[error("event subscriber limit is reached")]
    Busy,
    #[error("event stream is unavailable")]
    Unavailable,
}
