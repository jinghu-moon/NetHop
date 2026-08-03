use std::{fs, path::Path};

use nethop_android::ResourceCandidate;
use nethop_core::{CaptureMode, CapturePolicy};
use serde_json::{Map, Value};
use thiserror::Error;

const CONFIG_SCHEMA: &str = "nethop-worker-v1";
const MAX_CONFIG_BYTES: u64 = 64 * 1024;
const MAX_ALLOCATIONS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerConfig {
    capture: CapturePolicy,
    allocations: Vec<ResourceCandidate>,
}

impl WorkerConfig {
    pub fn load(path: &Path) -> Result<Self, WorkerConfigError> {
        if !path.is_absolute() {
            return Err(WorkerConfigError::InvalidPath);
        }
        let metadata = fs::symlink_metadata(path).map_err(|_| WorkerConfigError::InvalidPath)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() == 0
            || metadata.len() > MAX_CONFIG_BYTES
        {
            return Err(WorkerConfigError::InvalidPath);
        }
        let value: Value =
            serde_json::from_slice(&fs::read(path).map_err(|_| WorkerConfigError::InvalidPath)?)
                .map_err(|_| WorkerConfigError::InvalidJson)?;
        Self::from_value(value)
    }

    fn from_value(value: Value) -> Result<Self, WorkerConfigError> {
        let mut root = object(value)?;
        reject_unknown(
            &root,
            &[
                "schema",
                "inbound_port",
                "bypass_mark",
                "ipv6_guard",
                "include_uids",
                "exclude_uids",
                "allocations",
            ],
        )?;
        if take_string(&mut root, "schema")?.as_str() != CONFIG_SCHEMA {
            return Err(WorkerConfigError::UnsupportedSchema);
        }
        let inbound_port = take_u16(&mut root, "inbound_port")?;
        let bypass_mark = take_u32(&mut root, "bypass_mark")?;
        let ipv6_guard = take_bool(&mut root, "ipv6_guard")?;
        let include_uids = take_u32_array(&mut root, "include_uids")?;
        let exclude_uids = take_u32_array(&mut root, "exclude_uids")?;
        let allocation_values = root
            .remove("allocations")
            .and_then(|value| value.as_array().cloned())
            .ok_or(WorkerConfigError::InvalidField)?;
        if allocation_values.is_empty() || allocation_values.len() > MAX_ALLOCATIONS {
            return Err(WorkerConfigError::InvalidAllocation);
        }
        let mut allocations = Vec::with_capacity(allocation_values.len());
        for value in allocation_values {
            let mut allocation = object(value)?;
            reject_unknown(
                &allocation,
                &["mark", "mask", "route_table", "rule_priority"],
            )?;
            let candidate = ResourceCandidate::new(
                take_u32(&mut allocation, "mark")?,
                take_u32(&mut allocation, "mask")?,
                take_u32(&mut allocation, "route_table")?,
                take_u32(&mut allocation, "rule_priority")?,
            )
            .ok_or(WorkerConfigError::InvalidAllocation)?;
            if allocations.contains(&candidate) {
                return Err(WorkerConfigError::InvalidAllocation);
            }
            allocations.push(candidate);
        }
        let capture = CapturePolicy::new(
            CaptureMode::Tproxy,
            ipv6_guard,
            Some(inbound_port),
            Some(bypass_mark),
            include_uids,
            exclude_uids,
        )
        .map_err(|_| WorkerConfigError::InvalidCapture)?;
        Ok(Self {
            capture,
            allocations,
        })
    }

    pub const fn capture(&self) -> &CapturePolicy {
        &self.capture
    }

    pub fn allocations(&self) -> &[ResourceCandidate] {
        &self.allocations
    }
}

fn object(value: Value) -> Result<Map<String, Value>, WorkerConfigError> {
    value
        .as_object()
        .cloned()
        .ok_or(WorkerConfigError::InvalidField)
}

fn reject_unknown(object: &Map<String, Value>, allowed: &[&str]) -> Result<(), WorkerConfigError> {
    object
        .keys()
        .all(|key| allowed.contains(&key.as_str()))
        .then_some(())
        .ok_or(WorkerConfigError::UnknownField)
}

fn take_string(object: &mut Map<String, Value>, key: &str) -> Result<String, WorkerConfigError> {
    object
        .remove(key)
        .and_then(|value| value.as_str().map(str::to_owned))
        .filter(|value| !value.is_empty())
        .ok_or(WorkerConfigError::InvalidField)
}

fn take_bool(object: &mut Map<String, Value>, key: &str) -> Result<bool, WorkerConfigError> {
    object
        .remove(key)
        .and_then(|value| value.as_bool())
        .ok_or(WorkerConfigError::InvalidField)
}

fn take_u16(object: &mut Map<String, Value>, key: &str) -> Result<u16, WorkerConfigError> {
    let value = take_u32(object, key)?;
    u16::try_from(value)
        .ok()
        .filter(|value| *value != 0)
        .ok_or(WorkerConfigError::InvalidField)
}

fn take_u32(object: &mut Map<String, Value>, key: &str) -> Result<u32, WorkerConfigError> {
    object
        .remove(key)
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok())
        .ok_or(WorkerConfigError::InvalidField)
}

fn take_u32_array(
    object: &mut Map<String, Value>,
    key: &str,
) -> Result<Vec<u32>, WorkerConfigError> {
    object
        .remove(key)
        .and_then(|value| value.as_array().cloned())
        .ok_or(WorkerConfigError::InvalidField)?
        .into_iter()
        .map(|value| {
            value
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .ok_or(WorkerConfigError::InvalidField)
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum WorkerConfigError {
    #[error("worker config must be a bounded absolute regular non-symlink file")]
    InvalidPath,
    #[error("worker config is not valid JSON")]
    InvalidJson,
    #[error("worker config schema is unsupported")]
    UnsupportedSchema,
    #[error("worker config contains an unknown field")]
    UnknownField,
    #[error("worker config contains an invalid or missing field")]
    InvalidField,
    #[error("worker capture policy is invalid")]
    InvalidCapture,
    #[error("worker allocation candidate is invalid or duplicated")]
    InvalidAllocation,
}
