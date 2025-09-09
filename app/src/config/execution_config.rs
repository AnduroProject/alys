//! Execution client configuration

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for execution layer client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    /// HTTP endpoint URL for the execution client
    pub endpoint_url: String,
    
    /// Primary endpoint (compatibility alias) 
    pub endpoint: String,
    
    /// Fallback endpoints
    pub fallback_endpoints: Vec<String>,
    
    /// Cache size for various caches
    pub cache_size: usize,
    
    /// Request timeout in seconds
    pub request_timeout_secs: u64,
    
    /// Connection timeout in seconds  
    pub connection_timeout_secs: u64,
    
    /// Maximum number of retries for failed requests
    pub max_retries: u32,
    
    /// JWT secret for authentication (optional)
    pub jwt_secret: Option<String>,
    
    /// Enable metrics collection
    pub enable_metrics: bool,
    
    /// Health check interval in seconds
    pub health_check_interval_secs: u64,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        let endpoint_url = "http://127.0.0.1:8551".to_string();
        Self {
            endpoint_url: endpoint_url.clone(),
            endpoint: endpoint_url,
            fallback_endpoints: vec![],
            cache_size: 1000,
            request_timeout_secs: 30,
            connection_timeout_secs: 10,
            max_retries: 3,
            jwt_secret: None,
            enable_metrics: true,
            health_check_interval_secs: 30,
        }
    }
}