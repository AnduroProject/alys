//! Conversion utilities from Lighthouse v7 types to v4 types
//!
//! This module provides functions to convert between Lighthouse v7 and v4 types
//! for migration and compatibility purposes.

use crate::{error::FacadeResult, types::*};

/// Convert v7 ExecutionPayload to v4 format
/// 
/// Since v7 uses enum variants and v4 uses generic structs, we need to
/// extract the appropriate payload data and convert it to v4 format.
#[cfg(all(feature = "v4", feature = "v7"))]
pub fn convert_execution_payload(v7_payload: ExecutionPayload) -> FacadeResult<ExecutionPayload> {
    // For now, this is a mock implementation since v4 is disabled
    // In a real implementation, this would convert between the actual types
    Ok(v7_payload)
}

#[cfg(not(all(feature = "v4", feature = "v7")))]
pub fn convert_execution_payload(payload: ExecutionPayload) -> FacadeResult<ExecutionPayload> {
    // When not both features are enabled, just pass through
    Ok(payload)
}

/// Convert v7 PayloadStatus to v4 format
#[cfg(all(feature = "v4", feature = "v7"))]
pub fn convert_payload_status(v7_status: crate::types::PayloadStatus) -> FacadeResult<crate::types::PayloadStatus> {
    // Mock conversion - in real implementation would convert between v7::PayloadStatusV1 and v4::PayloadStatus
    Ok(v7_status)
}

#[cfg(not(all(feature = "v4", feature = "v7")))]
pub fn convert_payload_status(status: crate::types::PayloadStatus) -> FacadeResult<crate::types::PayloadStatus> {
    Ok(status)
}

/// Convert v7 ForkchoiceState to v4 format
#[cfg(all(feature = "v4", feature = "v7"))]
pub fn convert_forkchoice_state(v7_state: ForkchoiceState) -> FacadeResult<ForkchoiceState> {
    // Mock conversion - structures are similar between versions
    Ok(v7_state)
}

#[cfg(not(all(feature = "v4", feature = "v7")))]
pub fn convert_forkchoice_state(state: ForkchoiceState) -> FacadeResult<ForkchoiceState> {
    Ok(state)
}