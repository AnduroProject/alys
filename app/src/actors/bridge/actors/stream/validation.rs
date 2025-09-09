//! Configuration Validation System
//! 
//! Comprehensive validation rules and custom validators for StreamActor configuration

use std::path::Path;
use std::time::Duration;
use validator::{Validate, ValidationErrors, ValidationError};
use tracing::*;

use crate::config::{
    StreamConfig as AdvancedStreamConfig, StreamConfig as CoreStreamConfig, 
    TlsConfig as AdvancedConnectionConfig,
    AuthConfig as AuthenticationConfig, StreamConfig as MessagingConfig, 
    StreamConfig as PerformanceConfig,
    StreamConfig as FeatureConfig, Environment as EnvironmentType,
    GovernanceConfig as GovernanceEndpoint, TlsConfig, ReconnectionConfig as BackoffConfig,
};
use super::super::super::shared::errors::BridgeError;

/// Enhanced validation trait with context
pub trait EnhancedValidator {
    type Context;
    type Error;
    
    fn validate_with_context(&self, context: &Self::Context) -> Result<(), Self::Error>;
    fn validate_field_with_context(&self, field: &str, context: &Self::Context) -> Result<(), Self::Error>;
}

/// Validation context for configuration
#[derive(Debug, Clone)]
pub struct ValidationContext {
    pub environment: EnvironmentType,
    pub runtime_constraints: RuntimeConstraints,
    pub security_requirements: SecurityRequirements,
    pub feature_flags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RuntimeConstraints {
    pub max_memory_mb: u64,
    pub max_cpu_cores: u32,
    pub max_file_descriptors: u32,
    pub max_network_connections: u32,
}

#[derive(Debug, Clone)]
pub struct SecurityRequirements {
    pub require_tls: bool,
    pub require_auth: bool,
    pub require_mutual_tls: bool,
    pub min_key_size: u32,
    pub allowed_cipher_suites: Vec<String>,
}

impl Default for ValidationContext {
    fn default() -> Self {
        Self {
            environment: EnvironmentType::Development,
            runtime_constraints: RuntimeConstraints {
                max_memory_mb: 4096,
                max_cpu_cores: 8,
                max_file_descriptors: 1024,
                max_network_connections: 1000,
            },
            security_requirements: SecurityRequirements {
                require_tls: false,
                require_auth: false,
                require_mutual_tls: false,
                min_key_size: 2048,
                allowed_cipher_suites: vec![
                    "TLS_AES_256_GCM_SHA384".to_string(),
                    "TLS_CHACHA20_POLY1305_SHA256".to_string(),
                    "TLS_AES_128_GCM_SHA256".to_string(),
                ],
            },
            feature_flags: vec![],
        }
    }
}

// Implement Validate trait for all configuration structs
impl Validate for CoreStreamConfig {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        
        // Validate governance endpoints
        if self.governance_endpoints.is_empty() {
            let mut error = ValidationError::new("must_not_be_empty");
            error.message = Some("At least one governance endpoint must be configured".into());
            errors.add("governance_endpoints", error);
        }
        
        for (index, endpoint) in self.governance_endpoints.iter().enumerate() {
            if let Err(endpoint_errors) = endpoint.validate() {
                for (field, field_errors) in endpoint_errors.field_errors() {
                    for error in field_errors {
                        errors.add(&format!("governance_endpoints[{}].{}", index, field), error.clone());
                    }
                }
            }
        }
        
        // Validate actor ID
        if self.actor_id.is_empty() {
            let mut error = ValidationError::new("must_not_be_empty");
            error.message = Some("Actor ID cannot be empty".into());
            errors.add("actor_id", error);
        }
        
        if self.actor_id.len() > 64 {
            let mut error = ValidationError::new("max_length");
            error.message = Some("Actor ID cannot exceed 64 characters".into());
            errors.add("actor_id", error);
        }
        
        // Validate connection timeout
        if self.connection_timeout.as_secs() < 1 {
            let mut error = ValidationError::new("min_value");
            error.message = Some("Connection timeout must be at least 1 second".into());
            errors.add("connection_timeout", error);
        }
        
        if self.connection_timeout.as_secs() > 300 {
            let mut error = ValidationError::new("max_value");
            error.message = Some("Connection timeout cannot exceed 300 seconds".into());
            errors.add("connection_timeout", error);
        }
        
        // Validate heartbeat interval
        if self.heartbeat_interval.as_secs() < 5 {
            let mut error = ValidationError::new("min_value");
            error.message = Some("Heartbeat interval must be at least 5 seconds".into());
            errors.add("heartbeat_interval", error);
        }
        
        // Validate max connections
        if self.max_connections == 0 {
            let mut error = ValidationError::new("min_value");
            error.message = Some("Max connections must be greater than 0".into());
            errors.add("max_connections", error);
        }
        
        if self.max_connections > 10000 {
            let mut error = ValidationError::new("max_value");
            error.message = Some("Max connections cannot exceed 10000".into());
            errors.add("max_connections", error);
        }
        
        // Validate message buffer size
        if self.message_buffer_size == 0 {
            let mut error = ValidationError::new("min_value");
            error.message = Some("Message buffer size must be greater than 0".into());
            errors.add("message_buffer_size", error);
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Validate for GovernanceEndpoint {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        
        // Validate URL
        if self.url.is_empty() {
            let mut error = ValidationError::new("must_not_be_empty");
            error.message = Some("Endpoint URL cannot be empty".into());
            errors.add("url", error);
        } else {
            // Basic URL validation
            if !self.url.starts_with("http://") && !self.url.starts_with("https://") {
                let mut error = ValidationError::new("invalid_format");
                error.message = Some("URL must start with http:// or https://".into());
                errors.add("url", error);
            }
        }
        
        // Validate priority
        if self.priority > 100 {
            let mut error = ValidationError::new("max_value");
            error.message = Some("Priority cannot exceed 100".into());
            errors.add("priority", error);
        }
        
        // Validate expected latency
        if let Some(latency) = self.expected_latency_ms {
            if latency > 60000 {
                let mut error = ValidationError::new("max_value");
                error.message = Some("Expected latency cannot exceed 60 seconds".into());
                errors.add("expected_latency_ms", error);
            }
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Validate for AdvancedConnectionConfig {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        
        // Validate max connections
        if self.max_connections == 0 {
            let mut error = ValidationError::new("min_value");
            error.message = Some("Max connections must be greater than 0".into());
            errors.add("max_connections", error);
        }
        
        // Validate connection pool size
        if self.connection_pool_size > self.max_connections {
            let mut error = ValidationError::new("invalid_relationship");
            error.message = Some("Connection pool size cannot exceed max connections".into());
            errors.add("connection_pool_size", error);
        }
        
        // Validate timeouts
        if self.connection_timeout.as_millis() < 100 {
            let mut error = ValidationError::new("min_value");
            error.message = Some("Connection timeout must be at least 100ms".into());
            errors.add("connection_timeout", error);
        }
        
        if self.read_timeout.as_millis() < 100 {
            let mut error = ValidationError::new("min_value");
            error.message = Some("Read timeout must be at least 100ms".into());
            errors.add("read_timeout", error);
        }
        
        if self.write_timeout.as_millis() < 100 {
            let mut error = ValidationError::new("min_value");
            error.message = Some("Write timeout must be at least 100ms".into());
            errors.add("write_timeout", error);
        }
        
        // Validate heartbeat interval
        if self.heartbeat_interval.as_secs() < 1 {
            let mut error = ValidationError::new("min_value");
            error.message = Some("Heartbeat interval must be at least 1 second".into());
            errors.add("heartbeat_interval", error);
        }
        
        // Validate TLS configuration
        if let Err(tls_errors) = self.tls.validate() {
            for (field, field_errors) in tls_errors.field_errors() {
                for error in field_errors {
                    errors.add(&format!("tls.{}", field), error.clone());
                }
            }
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Validate for TlsConfig {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        
        if self.enabled {
            // Validate certificate paths if provided
            if let Some(ca_cert_path) = &self.ca_cert_path {
                if !Path::new(ca_cert_path).exists() {
                    let mut error = ValidationError::new("file_not_found");
                    error.message = Some(format!("CA certificate file not found: {}", ca_cert_path).into());
                    errors.add("ca_cert_path", error);
                }
            }
            
            if let Some(client_cert_path) = &self.client_cert_path {
                if !Path::new(client_cert_path).exists() {
                    let mut error = ValidationError::new("file_not_found");
                    error.message = Some(format!("Client certificate file not found: {}", client_cert_path).into());
                    errors.add("client_cert_path", error);
                }
            }
            
            if let Some(client_key_path) = &self.client_key_path {
                if !Path::new(client_key_path).exists() {
                    let mut error = ValidationError::new("file_not_found");
                    error.message = Some(format!("Client key file not found: {}", client_key_path).into());
                    errors.add("client_key_path", error);
                }
            }
            
            // Validate cipher suites
            if self.cipher_suites.is_empty() {
                let mut error = ValidationError::new("must_not_be_empty");
                error.message = Some("At least one cipher suite must be specified when TLS is enabled".into());
                errors.add("cipher_suites", error);
            }
            
            // Validate minimum TLS version
            match self.min_tls_version.as_str() {
                "1.2" | "1.3" => {},
                _ => {
                    let mut error = ValidationError::new("invalid_value");
                    error.message = Some("Minimum TLS version must be 1.2 or 1.3".into());
                    errors.add("min_tls_version", error);
                }
            }
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Validate for AuthenticationConfig {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        
        // Validate token refresh settings
        if self.token_refresh_enabled {
            if self.token_refresh_interval.as_secs() < 60 {
                let mut error = ValidationError::new("min_value");
                error.message = Some("Token refresh interval must be at least 60 seconds".into());
                errors.add("token_refresh_interval", error);
            }
            
            if self.token_refresh_buffer.as_secs() >= self.token_refresh_interval.as_secs() {
                let mut error = ValidationError::new("invalid_relationship");
                error.message = Some("Token refresh buffer must be less than refresh interval".into());
                errors.add("token_refresh_buffer", error);
            }
        }
        
        // Validate retry settings
        if self.auth_retry_attempts == 0 {
            let mut error = ValidationError::new("min_value");
            error.message = Some("Auth retry attempts must be greater than 0".into());
            errors.add("auth_retry_attempts", error);
        }
        
        if self.auth_retry_attempts > 10 {
            let mut error = ValidationError::new("max_value");
            error.message = Some("Auth retry attempts cannot exceed 10".into());
            errors.add("auth_retry_attempts", error);
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Validate for MessagingConfig {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        
        // Validate buffer sizes
        if self.message_buffer_size == 0 {
            let mut error = ValidationError::new("min_value");
            error.message = Some("Message buffer size must be greater than 0".into());
            errors.add("message_buffer_size", error);
        }
        
        if self.max_message_size == 0 {
            let mut error = ValidationError::new("min_value");
            error.message = Some("Max message size must be greater than 0".into());
            errors.add("max_message_size", error);
        }
        
        // Validate timeouts
        if self.request_timeout.as_millis() < 100 {
            let mut error = ValidationError::new("min_value");
            error.message = Some("Request timeout must be at least 100ms".into());
            errors.add("request_timeout", error);
        }
        
        if self.response_timeout.as_millis() < 100 {
            let mut error = ValidationError::new("min_value");
            error.message = Some("Response timeout must be at least 100ms".into());
            errors.add("response_timeout", error);
        }
        
        // Validate priority queue sizes
        if self.priority_queue_sizes.high == 0 {
            let mut error = ValidationError::new("min_value");
            error.message = Some("High priority queue size must be greater than 0".into());
            errors.add("priority_queue_sizes.high", error);
        }
        
        if self.priority_queue_sizes.normal == 0 {
            let mut error = ValidationError::new("min_value");
            error.message = Some("Normal priority queue size must be greater than 0".into());
            errors.add("priority_queue_sizes.normal", error);
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Validate for PerformanceConfig {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        
        // Validate thread counts
        if self.worker_threads == 0 {
            let mut error = ValidationError::new("min_value");
            error.message = Some("Worker threads must be greater than 0".into());
            errors.add("worker_threads", error);
        }
        
        if self.worker_threads > 256 {
            let mut error = ValidationError::new("max_value");
            error.message = Some("Worker threads cannot exceed 256".into());
            errors.add("worker_threads", error);
        }
        
        if self.blocking_threads == 0 {
            let mut error = ValidationError::new("min_value");
            error.message = Some("Blocking threads must be greater than 0".into());
            errors.add("blocking_threads", error);
        }
        
        // Validate memory settings
        if self.max_memory_usage_mb == 0 {
            let mut error = ValidationError::new("min_value");
            error.message = Some("Max memory usage must be greater than 0".into());
            errors.add("max_memory_usage_mb", error);
        }
        
        // Validate cache settings
        if self.message_cache_size == 0 {
            let mut error = ValidationError::new("min_value");
            error.message = Some("Message cache size must be greater than 0".into());
            errors.add("message_cache_size", error);
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Validate for FeatureConfig {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        
        // Validate A/B testing configuration
        if self.ab_testing_enabled {
            if self.ab_testing_config.test_percentage > 100 {
                let mut error = ValidationError::new("max_value");
                error.message = Some("A/B testing percentage cannot exceed 100".into());
                errors.add("ab_testing_config.test_percentage", error);
            }
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Validate for EnvironmentConfig {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        
        // Validate deployment region
        if self.deployment_region.is_empty() {
            let mut error = ValidationError::new("must_not_be_empty");
            error.message = Some("Deployment region cannot be empty".into());
            errors.add("deployment_region", error);
        }
        
        // Validate instance ID
        if self.instance_id.is_empty() {
            let mut error = ValidationError::new("must_not_be_empty");
            error.message = Some("Instance ID cannot be empty".into());
            errors.add("instance_id", error);
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

// Implement the main config validation
impl Validate for AdvancedStreamConfig {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        
        // Validate nested configs
        if let Err(core_errors) = self.core.validate() {
            for (field, field_errors) in core_errors.field_errors() {
                for error in field_errors {
                    errors.add(&format!("core.{}", field), error.clone());
                }
            }
        }
        
        if let Err(connection_errors) = self.connection.validate() {
            for (field, field_errors) in connection_errors.field_errors() {
                for error in field_errors {
                    errors.add(&format!("connection.{}", field), error.clone());
                }
            }
        }
        
        if let Err(auth_errors) = self.authentication.validate() {
            for (field, field_errors) in auth_errors.field_errors() {
                for error in field_errors {
                    errors.add(&format!("authentication.{}", field), error.clone());
                }
            }
        }
        
        if let Err(messaging_errors) = self.messaging.validate() {
            for (field, field_errors) in messaging_errors.field_errors() {
                for error in field_errors {
                    errors.add(&format!("messaging.{}", field), error.clone());
                }
            }
        }
        
        if let Err(performance_errors) = self.performance.validate() {
            for (field, field_errors) in performance_errors.field_errors() {
                for error in field_errors {
                    errors.add(&format!("performance.{}", field), error.clone());
                }
            }
        }
        
        if let Err(features_errors) = self.features.validate() {
            for (field, field_errors) in features_errors.field_errors() {
                for error in field_errors {
                    errors.add(&format!("features.{}", field), error.clone());
                }
            }
        }
        
        if let Err(environment_errors) = self.environment.validate() {
            for (field, field_errors) in environment_errors.field_errors() {
                for error in field_errors {
                    errors.add(&format!("environment.{}", field), error.clone());
                }
            }
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

// Enhanced validation with context
impl EnhancedValidator for AdvancedStreamConfig {
    type Context = ValidationContext;
    type Error = BridgeError;
    
    fn validate_with_context(&self, context: &Self::Context) -> Result<(), Self::Error> {
        // Run basic validation first
        self.validate()
            .map_err(|e| BridgeError::ValidationError(format!("Basic validation failed: {:?}", e)))?;
        
        // Environment-specific validation
        self.validate_for_environment(&context.environment)?;
        
        // Runtime constraints validation
        self.validate_runtime_constraints(&context.runtime_constraints)?;
        
        // Security requirements validation
        self.validate_security_requirements(&context.security_requirements)?;
        
        // Feature flag validation
        self.validate_feature_flags(&context.feature_flags)?;
        
        Ok(())
    }
    
    fn validate_field_with_context(&self, field: &str, context: &Self::Context) -> Result<(), Self::Error> {
        match field {
            "core" => self.core.validate().map_err(|e| BridgeError::ValidationError(format!("Core validation failed: {:?}", e))),
            "connection" => {
                self.connection.validate().map_err(|e| BridgeError::ValidationError(format!("Connection validation failed: {:?}", e)))?;
                self.validate_connection_for_environment(&context.environment)
            },
            "authentication" => {
                self.authentication.validate().map_err(|e| BridgeError::ValidationError(format!("Authentication validation failed: {:?}", e)))?;
                self.validate_auth_for_environment(&context.environment)
            },
            _ => Err(BridgeError::ValidationError(format!("Unknown field: {}", field))),
        }
    }
}

impl AdvancedStreamConfig {
    /// Validate configuration for specific environment
    fn validate_for_environment(&self, environment: &EnvironmentType) -> Result<(), BridgeError> {
        match environment {
            EnvironmentType::Production => {
                // Production-specific validation
                if !self.connection.tls.enabled {
                    return Err(BridgeError::ValidationError("TLS must be enabled in production".to_string()));
                }
                
                if self.authentication.auth_token.is_none() {
                    return Err(BridgeError::ValidationError("Authentication required in production".to_string()));
                }
                
                if self.features.debug_mode {
                    warn!("Debug mode enabled in production environment");
                }
                
                if self.features.experimental_protocols {
                    return Err(BridgeError::ValidationError("Experimental protocols not allowed in production".to_string()));
                }
            },
            EnvironmentType::Testing => {
                // Testing-specific validation
                if self.performance.max_memory_usage_mb > 1024 {
                    warn!("High memory usage configured for testing environment");
                }
            },
            EnvironmentType::Development => {
                // Development-specific validation (more lenient)
                if self.connection.tls.enabled && self.connection.tls.ca_cert_path.is_none() {
                    warn!("TLS enabled without CA certificate in development");
                }
            },
            _ => {}
        }
        
        Ok(())
    }
    
    /// Validate runtime constraints
    fn validate_runtime_constraints(&self, constraints: &RuntimeConstraints) -> Result<(), BridgeError> {
        if self.performance.max_memory_usage_mb > constraints.max_memory_mb {
            return Err(BridgeError::ValidationError(
                format!("Memory usage {} MB exceeds constraint {} MB", 
                    self.performance.max_memory_usage_mb, 
                    constraints.max_memory_mb)
            ));
        }
        
        if self.performance.worker_threads > constraints.max_cpu_cores {
            warn!("Worker threads ({}) exceed available CPU cores ({})", 
                self.performance.worker_threads, constraints.max_cpu_cores);
        }
        
        if self.connection.max_connections > constraints.max_network_connections as usize {
            return Err(BridgeError::ValidationError(
                format!("Max connections {} exceeds constraint {}", 
                    self.connection.max_connections, 
                    constraints.max_network_connections)
            ));
        }
        
        Ok(())
    }
    
    /// Validate security requirements
    fn validate_security_requirements(&self, requirements: &SecurityRequirements) -> Result<(), BridgeError> {
        if requirements.require_tls && !self.connection.tls.enabled {
            return Err(BridgeError::ValidationError("TLS is required by security policy".to_string()));
        }
        
        if requirements.require_auth && self.authentication.auth_token.is_none() {
            return Err(BridgeError::ValidationError("Authentication is required by security policy".to_string()));
        }
        
        if requirements.require_mutual_tls && (!self.connection.tls.enabled || !self.security.require_mutual_tls) {
            return Err(BridgeError::ValidationError("Mutual TLS is required by security policy".to_string()));
        }
        
        // Validate cipher suites
        if self.connection.tls.enabled {
            let allowed_ciphers: std::collections::HashSet<_> = requirements.allowed_cipher_suites.iter().collect();
            let configured_ciphers: std::collections::HashSet<_> = self.connection.tls.cipher_suites.iter().collect();
            
            if !configured_ciphers.is_subset(&allowed_ciphers) {
                return Err(BridgeError::ValidationError("Some configured cipher suites are not allowed by security policy".to_string()));
            }
        }
        
        Ok(())
    }
    
    /// Validate feature flags
    fn validate_feature_flags(&self, flags: &[String]) -> Result<(), BridgeError> {
        // Check if required feature flags are enabled
        if flags.contains(&"strict_validation".to_string()) {
            // Perform stricter validation
            if self.messaging.request_timeout < Duration::from_secs(10) {
                return Err(BridgeError::ValidationError("Request timeout too short for strict validation mode".to_string()));
            }
        }
        
        if flags.contains(&"high_availability".to_string()) {
            if self.connection.max_connections < 100 {
                return Err(BridgeError::ValidationError("Max connections too low for high availability mode".to_string()));
            }
        }
        
        Ok(())
    }
    
    /// Validate connection settings for environment
    fn validate_connection_for_environment(&self, environment: &EnvironmentType) -> Result<(), BridgeError> {
        match environment {
            EnvironmentType::Production => {
                if self.connection.connection_timeout < Duration::from_secs(30) {
                    warn!("Short connection timeout in production may cause instability");
                }
            },
            _ => {}
        }
        Ok(())
    }
    
    /// Validate authentication settings for environment
    fn validate_auth_for_environment(&self, environment: &EnvironmentType) -> Result<(), BridgeError> {
        match environment {
            EnvironmentType::Production => {
                if self.authentication.token_refresh_interval < Duration::from_secs(300) {
                    warn!("Short token refresh interval in production may cause overhead");
                }
            },
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_core_config_validation() {
        let mut config = CoreStreamConfig::default();
        assert!(config.validate().is_ok());
        
        // Test empty governance endpoints
        config.governance_endpoints.clear();
        assert!(config.validate().is_err());
        
        // Test empty actor ID
        config.governance_endpoints.push(GovernanceEndpoint::default());
        config.actor_id = "".to_string();
        assert!(config.validate().is_err());
    }
    
    #[test]
    fn test_advanced_config_validation_with_context() {
        let config = AdvancedStreamConfig::default();
        let context = ValidationContext::default();
        
        assert!(config.validate_with_context(&context).is_ok());
        
        // Test production environment requirements
        let mut prod_context = context.clone();
        prod_context.environment = EnvironmentType::Production;
        prod_context.security_requirements.require_tls = true;
        
        assert!(config.validate_with_context(&prod_context).is_err());
    }
    
    #[test]
    fn test_runtime_constraints_validation() {
        let config = AdvancedStreamConfig::default();
        let mut context = ValidationContext::default();
        
        // Set tight memory constraint
        context.runtime_constraints.max_memory_mb = 512;
        
        assert!(config.validate_with_context(&context).is_err());
    }
}