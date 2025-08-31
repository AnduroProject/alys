//! Signature Coordination for PegOut Operations
//! 
//! Coordinates multi-signature collection from governance nodes

use bitcoin::{Transaction, Witness};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tracing::{info, warn, debug};

use crate::actors::bridge::{
    messages::{SignatureSet, FederationSignature},
    shared::FederationConfig,
};
use super::actor::PegOutError;

/// Signature coordinator for multi-signature collection
#[derive(Debug)]
pub struct SignatureCoordinator {
    federation_config: FederationConfig,
    signature_timeout: Duration,
    pending_requests: HashMap<String, SignatureRequest>,
}

/// Signature request tracking
#[derive(Debug, Clone)]
pub struct SignatureRequest {
    pub request_id: String,
    pub transaction: Transaction,
    pub required_signatures: usize,
    pub collected_signatures: Vec<FederationSignature>,
    pub requested_at: SystemTime,
    pub status: SignatureRequestStatus,
}

/// Status of signature request
#[derive(Debug, Clone)]
pub enum SignatureRequestStatus {
    Pending,
    InProgress,
    Complete,
    Failed,
    Timeout,
}

impl SignatureCoordinator {
    /// Create new signature coordinator
    pub fn new(federation_config: FederationConfig, signature_timeout: Duration) -> Self {
        Self {
            federation_config,
            signature_timeout,
            pending_requests: HashMap::new(),
        }
    }

    /// Get required signatures count
    pub fn get_required_signatures(&self) -> usize {
        self.federation_config.threshold
    }

    /// Apply signatures to transaction
    pub fn apply_signatures(
        &self,
        unsigned_tx: &Transaction,
        signature_set: &SignatureSet,
    ) -> Result<Transaction, PegOutError> {
        info!("Applying {} signatures to transaction", signature_set.signatures.len());

        // Validate signature count
        if signature_set.signatures.len() < self.federation_config.threshold {
            return Err(PegOutError::SignatureError(format!(
                "Insufficient signatures: got {}, need {}",
                signature_set.signatures.len(),
                self.federation_config.threshold
            )));
        }

        // Create signed transaction
        let mut signed_tx = unsigned_tx.clone();

        // Apply witnesses to each input
        for (input_index, input) in signed_tx.input.iter_mut().enumerate() {
            let mut witness = Witness::new();
            
            // Add signatures for this input
            for sig in &signature_set.signatures {
                if sig.valid {
                    witness.push(&sig.signature);
                }
            }
            
            input.witness = witness;
        }

        debug!("Applied signatures to {} inputs", signed_tx.input.len());
        Ok(signed_tx)
    }

    /// Start signature request tracking
    pub fn start_request(&mut self, request_id: String, transaction: Transaction) {
        let request = SignatureRequest {
            request_id: request_id.clone(),
            transaction,
            required_signatures: self.federation_config.threshold,
            collected_signatures: Vec::new(),
            requested_at: SystemTime::now(),
            status: SignatureRequestStatus::Pending,
        };

        self.pending_requests.insert(request_id, request);
    }

    /// Add signature to request
    pub fn add_signature(
        &mut self,
        request_id: &str,
        signature: FederationSignature,
    ) -> Result<bool, PegOutError> {
        if let Some(request) = self.pending_requests.get_mut(request_id) {
            request.collected_signatures.push(signature);
            
            let is_complete = request.collected_signatures.len() >= request.required_signatures;
            if is_complete {
                request.status = SignatureRequestStatus::Complete;
            } else {
                request.status = SignatureRequestStatus::InProgress;
            }

            Ok(is_complete)
        } else {
            Err(PegOutError::SignatureError(format!("Unknown request: {}", request_id)))
        }
    }

    /// Check for timed out requests
    pub fn check_timeouts(&mut self) -> Vec<String> {
        let now = SystemTime::now();
        let mut timed_out = Vec::new();

        for (request_id, request) in &mut self.pending_requests {
            if matches!(request.status, SignatureRequestStatus::Pending | SignatureRequestStatus::InProgress) {
                if now.duration_since(request.requested_at).unwrap_or_default() > self.signature_timeout {
                    request.status = SignatureRequestStatus::Timeout;
                    timed_out.push(request_id.clone());
                }
            }
        }

        timed_out
    }

    /// Get request status
    pub fn get_request_status(&self, request_id: &str) -> Option<&SignatureRequest> {
        self.pending_requests.get(request_id)
    }

    /// Complete request and return signatures
    pub fn complete_request(&mut self, request_id: &str) -> Option<SignatureRequest> {
        self.pending_requests.remove(request_id)
    }
}