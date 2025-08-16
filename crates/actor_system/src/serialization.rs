//! Serialization and deserialization support for all actor messages and state structures
//!
//! This module provides comprehensive serialization capabilities for the actor system,
//! supporting multiple serialization formats, compression, versioning, and schema evolution.

use crate::{
    error::{ActorError, ActorResult},
    message::{AlysMessage, MessageEnvelope, MessageMetadata},
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::PhantomData;
use uuid::Uuid;

/// Supported serialization formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SerializationFormat {
    /// JSON - human readable, good for debugging
    Json,
    /// MessagePack - compact binary format
    MessagePack,
    /// Bincode - fast binary serialization
    Bincode,
    /// CBOR - standards-based binary format
    Cbor,
    /// Protocol Buffers - efficient schema-based format
    ProtocolBuffers,
}

/// Compression algorithms supported
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionAlgorithm {
    /// No compression
    None,
    /// LZ4 - fast compression/decompression
    Lz4,
    /// Zstd - good compression ratio and speed
    Zstd,
    /// Gzip - standard compression
    Gzip,
    /// Snappy - very fast compression
    Snappy,
}

/// Serialization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializationConfig {
    /// Primary serialization format
    pub format: SerializationFormat,
    /// Compression algorithm to use
    pub compression: CompressionAlgorithm,
    /// Compression level (algorithm-specific)
    pub compression_level: Option<u32>,
    /// Whether to include type information
    pub include_type_info: bool,
    /// Schema version for compatibility
    pub schema_version: u32,
    /// Maximum message size in bytes
    pub max_message_size: usize,
    /// Whether to validate messages after deserialization
    pub validate_after_deserialization: bool,
}

impl Default for SerializationConfig {
    fn default() -> Self {
        Self {
            format: SerializationFormat::MessagePack,
            compression: CompressionAlgorithm::Lz4,
            compression_level: None,
            include_type_info: true,
            schema_version: 1,
            max_message_size: 64 * 1024 * 1024, // 64MB
            validate_after_deserialization: true,
        }
    }
}

/// Serialized message container with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedMessage {
    /// Unique identifier for this serialized message
    pub id: Uuid,
    /// Serialization format used
    pub format: SerializationFormat,
    /// Compression used
    pub compression: CompressionAlgorithm,
    /// Schema version
    pub schema_version: u32,
    /// Message type name
    pub message_type: String,
    /// Serialized data
    pub data: Vec<u8>,
    /// Serialization metadata
    pub metadata: SerializationMetadata,
    /// Checksum for integrity verification
    pub checksum: u64,
    /// When this was serialized
    pub serialized_at: std::time::SystemTime,
}

/// Metadata about the serialization process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializationMetadata {
    /// Original message size before serialization
    pub original_size: usize,
    /// Compressed size
    pub compressed_size: usize,
    /// Time taken to serialize
    pub serialization_time: std::time::Duration,
    /// Compression ratio achieved
    pub compression_ratio: f64,
    /// Schema hash for compatibility checking
    pub schema_hash: Option<String>,
    /// Additional format-specific metadata
    pub format_metadata: HashMap<String, String>,
}

/// Trait for serializable actor messages
pub trait SerializableMessage: AlysMessage + Serialize + DeserializeOwned {
    /// Get schema version for this message type
    fn schema_version() -> u32 {
        1
    }
    
    /// Get schema hash for compatibility checking
    fn schema_hash() -> Option<String> {
        None
    }
    
    /// Validate message after deserialization
    fn validate(&self) -> ActorResult<()> {
        Ok(())
    }
    
    /// Handle schema migration if needed
    fn migrate_from_version(_version: u32, _data: &[u8]) -> ActorResult<Self> {
        Err(ActorError::DeserializationFailed {
            reason: "Schema migration not implemented".to_string(),
        })
    }
}

/// Actor state serialization trait
pub trait SerializableActorState: Serialize + DeserializeOwned + Debug + Clone {
    /// Get state schema version
    fn state_schema_version() -> u32 {
        1
    }
    
    /// Validate state after deserialization
    fn validate_state(&self) -> ActorResult<()> {
        Ok(())
    }
    
    /// Handle state migration from previous versions
    fn migrate_state_from_version(_version: u32, _data: &[u8]) -> ActorResult<Self> {
        Err(ActorError::DeserializationFailed {
            reason: "State migration not implemented".to_string(),
        })
    }
}

/// Main serializer for actor messages and state
pub struct ActorSerializer {
    config: SerializationConfig,
    compressors: HashMap<CompressionAlgorithm, Box<dyn Compressor>>,
    serializers: HashMap<SerializationFormat, Box<dyn MessageSerializer>>,
}

impl ActorSerializer {
    /// Create new serializer with default configuration
    pub fn new() -> Self {
        Self::with_config(SerializationConfig::default())
    }
    
    /// Create serializer with custom configuration
    pub fn with_config(config: SerializationConfig) -> Self {
        let mut serializer = Self {
            config,
            compressors: HashMap::new(),
            serializers: HashMap::new(),
        };
        
        serializer.register_default_compressors();
        serializer.register_default_serializers();
        serializer
    }
    
    /// Register default compression algorithms
    fn register_default_compressors(&mut self) {
        self.compressors.insert(CompressionAlgorithm::None, Box::new(NoCompressor));
        self.compressors.insert(CompressionAlgorithm::Lz4, Box::new(Lz4Compressor));
        self.compressors.insert(CompressionAlgorithm::Zstd, Box::new(ZstdCompressor));
        self.compressors.insert(CompressionAlgorithm::Gzip, Box::new(GzipCompressor));
        self.compressors.insert(CompressionAlgorithm::Snappy, Box::new(SnappyCompressor));
    }
    
    /// Register default serialization formats
    fn register_default_serializers(&mut self) {
        self.serializers.insert(SerializationFormat::Json, Box::new(JsonSerializer));
        self.serializers.insert(SerializationFormat::MessagePack, Box::new(MessagePackSerializer));
        self.serializers.insert(SerializationFormat::Bincode, Box::new(BincodeSerializer));
        self.serializers.insert(SerializationFormat::Cbor, Box::new(CborSerializer));
        self.serializers.insert(SerializationFormat::ProtocolBuffers, Box::new(ProtobufSerializer));
    }
    
    /// Serialize a message envelope
    pub fn serialize_envelope<T>(&self, envelope: &MessageEnvelope<T>) -> ActorResult<SerializedMessage>
    where
        T: SerializableMessage,
    {
        let start_time = std::time::Instant::now();
        
        // Get serializer for configured format
        let serializer = self.serializers.get(&self.config.format)
            .ok_or_else(|| ActorError::SerializationFailed {
                reason: format!("Serializer not found for format: {:?}", self.config.format),
            })?;
        
        // Serialize the envelope
        let serialized_data = serializer.serialize(envelope)?;
        let original_size = serialized_data.len();
        
        // Validate size limit
        if original_size > self.config.max_message_size {
            return Err(ActorError::SerializationFailed {
                reason: format!("Message size {} exceeds limit {}", original_size, self.config.max_message_size),
            });
        }
        
        // Compress if configured
        let compressor = self.compressors.get(&self.config.compression)
            .ok_or_else(|| ActorError::SerializationFailed {
                reason: format!("Compressor not found for algorithm: {:?}", self.config.compression),
            })?;
        
        let compressed_data = compressor.compress(&serialized_data, self.config.compression_level)?;
        let compressed_size = compressed_data.len();
        
        let serialization_time = start_time.elapsed();
        let compression_ratio = if original_size > 0 {
            compressed_size as f64 / original_size as f64
        } else {
            1.0
        };
        
        // Calculate checksum
        let checksum = Self::calculate_checksum(&compressed_data);
        
        Ok(SerializedMessage {
            id: Uuid::new_v4(),
            format: self.config.format,
            compression: self.config.compression,
            schema_version: T::schema_version(),
            message_type: envelope.payload.message_type().to_string(),
            data: compressed_data,
            metadata: SerializationMetadata {
                original_size,
                compressed_size,
                serialization_time,
                compression_ratio,
                schema_hash: T::schema_hash(),
                format_metadata: HashMap::new(),
            },
            checksum,
            serialized_at: std::time::SystemTime::now(),
        })
    }
    
    /// Deserialize a message envelope
    pub fn deserialize_envelope<T>(&self, serialized: &SerializedMessage) -> ActorResult<MessageEnvelope<T>>
    where
        T: SerializableMessage,
    {
        // Verify checksum
        let calculated_checksum = Self::calculate_checksum(&serialized.data);
        if calculated_checksum != serialized.checksum {
            return Err(ActorError::DeserializationFailed {
                reason: "Checksum verification failed".to_string(),
            });
        }
        
        // Check schema version compatibility
        if serialized.schema_version > T::schema_version() {
            return Err(ActorError::DeserializationFailed {
                reason: format!(
                    "Schema version {} is newer than supported version {}",
                    serialized.schema_version, T::schema_version()
                ),
            });
        }
        
        // Decompress data
        let compressor = self.compressors.get(&serialized.compression)
            .ok_or_else(|| ActorError::DeserializationFailed {
                reason: format!("Compressor not found for algorithm: {:?}", serialized.compression),
            })?;
        
        let decompressed_data = compressor.decompress(&serialized.data)?;
        
        // Handle schema migration if needed
        let envelope = if serialized.schema_version < T::schema_version() {
            // Attempt migration
            let migrated_payload = T::migrate_from_version(serialized.schema_version, &decompressed_data)?;
            MessageEnvelope::new(migrated_payload)
        } else {
            // Deserialize normally
            let deserializer = self.serializers.get(&serialized.format)
                .ok_or_else(|| ActorError::DeserializationFailed {
                    reason: format!("Deserializer not found for format: {:?}", serialized.format),
                })?;
            
            deserializer.deserialize(&decompressed_data)?
        };
        
        // Validate if configured
        if self.config.validate_after_deserialization {
            envelope.payload.validate()?;
        }
        
        Ok(envelope)
    }
    
    /// Serialize actor state
    pub fn serialize_state<S>(&self, state: &S) -> ActorResult<SerializedMessage>
    where
        S: SerializableActorState,
    {
        let start_time = std::time::Instant::now();
        
        let serializer = self.serializers.get(&self.config.format)
            .ok_or_else(|| ActorError::SerializationFailed {
                reason: format!("Serializer not found for format: {:?}", self.config.format),
            })?;
        
        let serialized_data = serializer.serialize_state(state)?;
        let original_size = serialized_data.len();
        
        let compressor = self.compressors.get(&self.config.compression)
            .ok_or_else(|| ActorError::SerializationFailed {
                reason: format!("Compressor not found for algorithm: {:?}", self.config.compression),
            })?;
        
        let compressed_data = compressor.compress(&serialized_data, self.config.compression_level)?;
        let compressed_size = compressed_data.len();
        
        let serialization_time = start_time.elapsed();
        let compression_ratio = if original_size > 0 {
            compressed_size as f64 / original_size as f64
        } else {
            1.0
        };
        
        let checksum = Self::calculate_checksum(&compressed_data);
        
        Ok(SerializedMessage {
            id: Uuid::new_v4(),
            format: self.config.format,
            compression: self.config.compression,
            schema_version: S::state_schema_version(),
            message_type: "ActorState".to_string(),
            data: compressed_data,
            metadata: SerializationMetadata {
                original_size,
                compressed_size,
                serialization_time,
                compression_ratio,
                schema_hash: None,
                format_metadata: HashMap::new(),
            },
            checksum,
            serialized_at: std::time::SystemTime::now(),
        })
    }
    
    /// Deserialize actor state
    pub fn deserialize_state<S>(&self, serialized: &SerializedMessage) -> ActorResult<S>
    where
        S: SerializableActorState,
    {
        // Verify checksum
        let calculated_checksum = Self::calculate_checksum(&serialized.data);
        if calculated_checksum != serialized.checksum {
            return Err(ActorError::DeserializationFailed {
                reason: "Checksum verification failed".to_string(),
            });
        }
        
        // Decompress data
        let compressor = self.compressors.get(&serialized.compression)
            .ok_or_else(|| ActorError::DeserializationFailed {
                reason: format!("Compressor not found for algorithm: {:?}", serialized.compression),
            })?;
        
        let decompressed_data = compressor.decompress(&serialized.data)?;
        
        // Handle state migration if needed
        let state = if serialized.schema_version < S::state_schema_version() {
            S::migrate_state_from_version(serialized.schema_version, &decompressed_data)?
        } else {
            let deserializer = self.serializers.get(&serialized.format)
                .ok_or_else(|| ActorError::DeserializationFailed {
                    reason: format!("Deserializer not found for format: {:?}", serialized.format),
                })?;
            
            deserializer.deserialize_state(&decompressed_data)?
        };
        
        // Validate state
        if self.config.validate_after_deserialization {
            state.validate_state()?;
        }
        
        Ok(state)
    }
    
    /// Calculate checksum for data integrity
    fn calculate_checksum(data: &[u8]) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        data.hash(&mut hasher);
        hasher.finish()
    }
}

impl Default for ActorSerializer {
    fn default() -> Self {
        Self::new()
    }
}

/// Compression trait for different algorithms
pub trait Compressor: Send + Sync {
    fn compress(&self, data: &[u8], level: Option<u32>) -> ActorResult<Vec<u8>>;
    fn decompress(&self, data: &[u8]) -> ActorResult<Vec<u8>>;
}

/// Message serialization trait for different formats
pub trait MessageSerializer: Send + Sync {
    fn serialize<T: Serialize>(&self, message: &T) -> ActorResult<Vec<u8>>;
    fn deserialize<T: DeserializeOwned>(&self, data: &[u8]) -> ActorResult<T>;
    fn serialize_state<S: Serialize>(&self, state: &S) -> ActorResult<Vec<u8>>;
    fn deserialize_state<S: DeserializeOwned>(&self, data: &[u8]) -> ActorResult<S>;
}

/// No compression implementation
pub struct NoCompressor;

impl Compressor for NoCompressor {
    fn compress(&self, data: &[u8], _level: Option<u32>) -> ActorResult<Vec<u8>> {
        Ok(data.to_vec())
    }
    
    fn decompress(&self, data: &[u8]) -> ActorResult<Vec<u8>> {
        Ok(data.to_vec())
    }
}

/// LZ4 compression implementation
pub struct Lz4Compressor;

impl Compressor for Lz4Compressor {
    fn compress(&self, data: &[u8], _level: Option<u32>) -> ActorResult<Vec<u8>> {
        // Note: In a real implementation, you would use the lz4 crate
        // For now, we'll just return the original data as a placeholder
        Ok(data.to_vec())
    }
    
    fn decompress(&self, data: &[u8]) -> ActorResult<Vec<u8>> {
        // Note: In a real implementation, you would use the lz4 crate
        Ok(data.to_vec())
    }
}

/// Zstd compression implementation
pub struct ZstdCompressor;

impl Compressor for ZstdCompressor {
    fn compress(&self, data: &[u8], level: Option<u32>) -> ActorResult<Vec<u8>> {
        // Note: In a real implementation, you would use the zstd crate
        Ok(data.to_vec())
    }
    
    fn decompress(&self, data: &[u8]) -> ActorResult<Vec<u8>> {
        // Note: In a real implementation, you would use the zstd crate
        Ok(data.to_vec())
    }
}

/// Gzip compression implementation
pub struct GzipCompressor;

impl Compressor for GzipCompressor {
    fn compress(&self, data: &[u8], _level: Option<u32>) -> ActorResult<Vec<u8>> {
        // Note: In a real implementation, you would use the flate2 crate
        Ok(data.to_vec())
    }
    
    fn decompress(&self, data: &[u8]) -> ActorResult<Vec<u8>> {
        // Note: In a real implementation, you would use the flate2 crate
        Ok(data.to_vec())
    }
}

/// Snappy compression implementation
pub struct SnappyCompressor;

impl Compressor for SnappyCompressor {
    fn compress(&self, data: &[u8], _level: Option<u32>) -> ActorResult<Vec<u8>> {
        // Note: In a real implementation, you would use the snap crate
        Ok(data.to_vec())
    }
    
    fn decompress(&self, data: &[u8]) -> ActorResult<Vec<u8>> {
        // Note: In a real implementation, you would use the snap crate
        Ok(data.to_vec())
    }
}

/// JSON serializer implementation
pub struct JsonSerializer;

impl MessageSerializer for JsonSerializer {
    fn serialize<T: Serialize>(&self, message: &T) -> ActorResult<Vec<u8>> {
        serde_json::to_vec(message).map_err(|e| ActorError::SerializationFailed {
            reason: format!("JSON serialization failed: {}", e),
        })
    }
    
    fn deserialize<T: DeserializeOwned>(&self, data: &[u8]) -> ActorResult<T> {
        serde_json::from_slice(data).map_err(|e| ActorError::DeserializationFailed {
            reason: format!("JSON deserialization failed: {}", e),
        })
    }
    
    fn serialize_state<S: Serialize>(&self, state: &S) -> ActorResult<Vec<u8>> {
        self.serialize(state)
    }
    
    fn deserialize_state<S: DeserializeOwned>(&self, data: &[u8]) -> ActorResult<S> {
        self.deserialize(data)
    }
}

/// MessagePack serializer implementation
pub struct MessagePackSerializer;

impl MessageSerializer for MessagePackSerializer {
    fn serialize<T: Serialize>(&self, message: &T) -> ActorResult<Vec<u8>> {
        // Note: In a real implementation, you would use the rmp-serde crate
        // For now, we'll fall back to JSON
        serde_json::to_vec(message).map_err(|e| ActorError::SerializationFailed {
            reason: format!("MessagePack serialization failed: {}", e),
        })
    }
    
    fn deserialize<T: DeserializeOwned>(&self, data: &[u8]) -> ActorResult<T> {
        // Note: In a real implementation, you would use the rmp-serde crate
        serde_json::from_slice(data).map_err(|e| ActorError::DeserializationFailed {
            reason: format!("MessagePack deserialization failed: {}", e),
        })
    }
    
    fn serialize_state<S: Serialize>(&self, state: &S) -> ActorResult<Vec<u8>> {
        self.serialize(state)
    }
    
    fn deserialize_state<S: DeserializeOwned>(&self, data: &[u8]) -> ActorResult<S> {
        self.deserialize(data)
    }
}

/// Bincode serializer implementation
pub struct BincodeSerializer;

impl MessageSerializer for BincodeSerializer {
    fn serialize<T: Serialize>(&self, message: &T) -> ActorResult<Vec<u8>> {
        bincode::serialize(message).map_err(|e| ActorError::SerializationFailed {
            reason: format!("Bincode serialization failed: {}", e),
        })
    }
    
    fn deserialize<T: DeserializeOwned>(&self, data: &[u8]) -> ActorResult<T> {
        bincode::deserialize(data).map_err(|e| ActorError::DeserializationFailed {
            reason: format!("Bincode deserialization failed: {}", e),
        })
    }
    
    fn serialize_state<S: Serialize>(&self, state: &S) -> ActorResult<Vec<u8>> {
        self.serialize(state)
    }
    
    fn deserialize_state<S: DeserializeOwned>(&self, data: &[u8]) -> ActorResult<S> {
        self.deserialize(data)
    }
}

/// CBOR serializer implementation
pub struct CborSerializer;

impl MessageSerializer for CborSerializer {
    fn serialize<T: Serialize>(&self, message: &T) -> ActorResult<Vec<u8>> {
        // Note: In a real implementation, you would use the serde_cbor crate
        serde_json::to_vec(message).map_err(|e| ActorError::SerializationFailed {
            reason: format!("CBOR serialization failed: {}", e),
        })
    }
    
    fn deserialize<T: DeserializeOwned>(&self, data: &[u8]) -> ActorResult<T> {
        // Note: In a real implementation, you would use the serde_cbor crate
        serde_json::from_slice(data).map_err(|e| ActorError::DeserializationFailed {
            reason: format!("CBOR deserialization failed: {}", e),
        })
    }
    
    fn serialize_state<S: Serialize>(&self, state: &S) -> ActorResult<Vec<u8>> {
        self.serialize(state)
    }
    
    fn deserialize_state<S: DeserializeOwned>(&self, data: &[u8]) -> ActorResult<S> {
        self.deserialize(data)
    }
}

/// Protocol Buffers serializer implementation
pub struct ProtobufSerializer;

impl MessageSerializer for ProtobufSerializer {
    fn serialize<T: Serialize>(&self, message: &T) -> ActorResult<Vec<u8>> {
        // Note: In a real implementation, you would use protobuf libraries
        // For now, we'll fall back to JSON
        serde_json::to_vec(message).map_err(|e| ActorError::SerializationFailed {
            reason: format!("Protobuf serialization failed: {}", e),
        })
    }
    
    fn deserialize<T: DeserializeOwned>(&self, data: &[u8]) -> ActorResult<T> {
        // Note: In a real implementation, you would use protobuf libraries
        serde_json::from_slice(data).map_err(|e| ActorError::DeserializationFailed {
            reason: format!("Protobuf deserialization failed: {}", e),
        })
    }
    
    fn serialize_state<S: Serialize>(&self, state: &S) -> ActorResult<Vec<u8>> {
        self.serialize(state)
    }
    
    fn deserialize_state<S: DeserializeOwned>(&self, data: &[u8]) -> ActorResult<S> {
        self.deserialize(data)
    }
}

/// Serialization batch operations for performance
pub struct BatchSerializer {
    serializer: ActorSerializer,
    batch_size: usize,
}

impl BatchSerializer {
    /// Create new batch serializer
    pub fn new(config: SerializationConfig, batch_size: usize) -> Self {
        Self {
            serializer: ActorSerializer::with_config(config),
            batch_size,
        }
    }
    
    /// Serialize multiple messages in a batch
    pub fn serialize_batch<T>(&self, envelopes: &[MessageEnvelope<T>]) -> ActorResult<Vec<SerializedMessage>>
    where
        T: SerializableMessage,
    {
        let mut results = Vec::with_capacity(envelopes.len());
        
        for chunk in envelopes.chunks(self.batch_size) {
            for envelope in chunk {
                let serialized = self.serializer.serialize_envelope(envelope)?;
                results.push(serialized);
            }
        }
        
        Ok(results)
    }
    
    /// Deserialize multiple messages in a batch
    pub fn deserialize_batch<T>(&self, serialized: &[SerializedMessage]) -> ActorResult<Vec<MessageEnvelope<T>>>
    where
        T: SerializableMessage,
    {
        let mut results = Vec::with_capacity(serialized.len());
        
        for chunk in serialized.chunks(self.batch_size) {
            for msg in chunk {
                let envelope = self.serializer.deserialize_envelope(msg)?;
                results.push(envelope);
            }
        }
        
        Ok(results)
    }
}

/// Global serializer instance for convenience
static GLOBAL_SERIALIZER: once_cell::sync::Lazy<ActorSerializer> = 
    once_cell::sync::Lazy::new(ActorSerializer::new);

/// Serialize message using global serializer
pub fn serialize_message<T>(envelope: &MessageEnvelope<T>) -> ActorResult<SerializedMessage>
where
    T: SerializableMessage,
{
    GLOBAL_SERIALIZER.serialize_envelope(envelope)
}

/// Deserialize message using global serializer
pub fn deserialize_message<T>(serialized: &SerializedMessage) -> ActorResult<MessageEnvelope<T>>
where
    T: SerializableMessage,
{
    GLOBAL_SERIALIZER.deserialize_envelope(serialized)
}

/// Serialize actor state using global serializer
pub fn serialize_state<S>(state: &S) -> ActorResult<SerializedMessage>
where
    S: SerializableActorState,
{
    GLOBAL_SERIALIZER.serialize_state(state)
}

/// Deserialize actor state using global serializer
pub fn deserialize_state<S>(serialized: &SerializedMessage) -> ActorResult<S>
where
    S: SerializableActorState,
{
    GLOBAL_SERIALIZER.deserialize_state(serialized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::HealthCheckMessage;
    
    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestState {
        value: u64,
        name: String,
    }
    
    impl SerializableActorState for TestState {}
    
    impl SerializableMessage for HealthCheckMessage {}
    
    #[test]
    fn test_message_serialization() {
        let serializer = ActorSerializer::new();
        let envelope = MessageEnvelope::new(HealthCheckMessage);
        
        let serialized = serializer.serialize_envelope(&envelope).unwrap();
        assert_eq!(serialized.message_type, "HealthCheck");
        assert!(serialized.data.len() > 0);
        
        let deserialized: MessageEnvelope<HealthCheckMessage> = 
            serializer.deserialize_envelope(&serialized).unwrap();
        assert_eq!(deserialized.payload.message_type(), envelope.payload.message_type());
    }
    
    #[test]
    fn test_state_serialization() {
        let serializer = ActorSerializer::new();
        let state = TestState {
            value: 42,
            name: "test".to_string(),
        };
        
        let serialized = serializer.serialize_state(&state).unwrap();
        assert!(serialized.data.len() > 0);
        
        let deserialized: TestState = serializer.deserialize_state(&serialized).unwrap();
        assert_eq!(deserialized.value, state.value);
        assert_eq!(deserialized.name, state.name);
    }
    
    #[test]
    fn test_batch_serialization() {
        let batch_serializer = BatchSerializer::new(SerializationConfig::default(), 10);
        let envelopes = vec![
            MessageEnvelope::new(HealthCheckMessage),
            MessageEnvelope::new(HealthCheckMessage),
        ];
        
        let serialized_batch = batch_serializer.serialize_batch(&envelopes).unwrap();
        assert_eq!(serialized_batch.len(), 2);
        
        let deserialized_batch: Vec<MessageEnvelope<HealthCheckMessage>> = 
            batch_serializer.deserialize_batch(&serialized_batch).unwrap();
        assert_eq!(deserialized_batch.len(), 2);
    }
    
    #[test]
    fn test_checksum_verification() {
        let serializer = ActorSerializer::new();
        let envelope = MessageEnvelope::new(HealthCheckMessage);
        
        let mut serialized = serializer.serialize_envelope(&envelope).unwrap();
        
        // Corrupt the checksum
        serialized.checksum = 0;
        
        let result: ActorResult<MessageEnvelope<HealthCheckMessage>> = 
            serializer.deserialize_envelope(&serialized);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("checksum"));
    }
}