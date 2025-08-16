//! Serde utilities for common types

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Serde module for Duration serialization
pub mod duration_serde {
    use super::*;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_nanos().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let nanos = u128::deserialize(deserializer)?;
        Ok(Duration::from_nanos(nanos as u64))
    }
}

/// Serde module for SystemTime serialization
pub mod systemtime_serde {
    use super::*;

    pub fn serialize<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let duration_since_epoch = time.duration_since(UNIX_EPOCH)
            .map_err(|_| serde::ser::Error::custom("SystemTime before UNIX_EPOCH"))?;
        duration_since_epoch.as_nanos().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let nanos = u128::deserialize(deserializer)?;
        let duration = Duration::from_nanos(nanos as u64);
        Ok(UNIX_EPOCH + duration)
    }
}