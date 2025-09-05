//! Response conversion utilities between Lighthouse versions
//!
//! This module provides functions to convert response types between different
//! Lighthouse versions for seamless operation during migration.

use crate::{error::FacadeResult, types::*};

/// Convert PayloadStatus from v4 to unified format
#[cfg(feature = "v4")]
pub fn convert_payload_status_from_v4(v4_status: crate::types::PayloadStatus) -> FacadeResult<crate::types::PayloadStatus> {
    // Since we're using a unified type system, this is mostly a pass-through
    // In a real implementation with different v4/v7 types, this would do actual conversion
    Ok(v4_status)
}

#[cfg(not(feature = "v4"))]
pub fn convert_payload_status_from_v4(status: crate::types::PayloadStatus) -> FacadeResult<crate::types::PayloadStatus> {
    Ok(status)
}

/// Convert PayloadStatus from v7 to unified format  
#[cfg(feature = "v7")]
pub fn convert_payload_status_from_v7(v7_status: crate::types::PayloadStatus) -> FacadeResult<crate::types::PayloadStatus> {
    // Pass through since we're using v7 types as the unified format
    Ok(v7_status)
}

#[cfg(not(feature = "v7"))]
pub fn convert_payload_status_from_v7(status: crate::types::PayloadStatus) -> FacadeResult<crate::types::PayloadStatus> {
    Ok(status)
}

/// Convert GetPayloadResponse from v4 to unified format
#[cfg(feature = "v4")]
pub fn convert_get_payload_response_from_v4(v4_response: GetPayloadResponse) -> FacadeResult<GetPayloadResponse> {
    Ok(v4_response)
}

#[cfg(not(feature = "v4"))]
pub fn convert_get_payload_response_from_v4(response: GetPayloadResponse) -> FacadeResult<GetPayloadResponse> {
    Ok(response)
}

/// Convert GetPayloadResponse from v7 to unified format
#[cfg(feature = "v7")]
pub fn convert_get_payload_response_from_v7(v7_response: GetPayloadResponse) -> FacadeResult<GetPayloadResponse> {
    Ok(v7_response)
}

#[cfg(not(feature = "v7"))]
pub fn convert_get_payload_response_from_v7(response: GetPayloadResponse) -> FacadeResult<GetPayloadResponse> {
    Ok(response)
}

/// Convert ForkchoiceUpdatedResponse from v4 to unified format
#[cfg(feature = "v4")]  
pub fn convert_forkchoice_updated_response_from_v4(v4_response: ForkchoiceUpdatedResponse) -> FacadeResult<ForkchoiceUpdatedResponse> {
    Ok(v4_response)
}

#[cfg(not(feature = "v4"))]
pub fn convert_forkchoice_updated_response_from_v4(response: ForkchoiceUpdatedResponse) -> FacadeResult<ForkchoiceUpdatedResponse> {
    Ok(response)
}

/// Convert ForkchoiceUpdatedResponse from v7 to unified format
#[cfg(feature = "v7")]
pub fn convert_forkchoice_updated_response_from_v7(v7_response: ForkchoiceUpdatedResponse) -> FacadeResult<ForkchoiceUpdatedResponse> {
    Ok(v7_response)
}

#[cfg(not(feature = "v7"))]
pub fn convert_forkchoice_updated_response_from_v7(response: ForkchoiceUpdatedResponse) -> FacadeResult<ForkchoiceUpdatedResponse> {
    Ok(response)
}