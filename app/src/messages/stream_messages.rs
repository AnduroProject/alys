//! Real-time streaming and WebSocket messages

use crate::types::*;
use actix::prelude::*;

/// Message to handle new WebSocket connection
#[derive(Message)]
#[rtype(result = "Result<(), StreamError>")]
pub struct NewConnectionMessage {
    pub connection_id: String,
    pub client_address: String,
    pub auth_token: Option<String>,
}

/// Message to handle connection disconnection
#[derive(Message)]
#[rtype(result = "Result<(), StreamError>")]
pub struct DisconnectionMessage {
    pub connection_id: String,
}

/// Message to subscribe connection to a topic
#[derive(Message)]
#[rtype(result = "Result<(), StreamError>")]
pub struct SubscribeMessage {
    pub connection_id: String,
    pub topic: String,
    pub filters: Option<SubscriptionFilters>,
}

/// Message to unsubscribe connection from a topic
#[derive(Message)]
#[rtype(result = "Result<(), StreamError>")]
pub struct UnsubscribeMessage {
    pub connection_id: String,
    pub topic: String,
}

/// Message to broadcast data to all subscribers of a topic
#[derive(Message)]
#[rtype(result = "Result<(), StreamError>")]
pub struct BroadcastMessage {
    pub message: StreamMessage,
}

/// Message to send data to a specific connection
#[derive(Message)]
#[rtype(result = "Result<(), StreamError>")]
pub struct SendToConnectionMessage {
    pub connection_id: String,
    pub message: StreamMessage,
}

/// Message to handle block events for streaming
#[derive(Message)]
#[rtype(result = "Result<(), StreamError>")]
pub struct BlockEventMessage {
    pub block: ConsensusBlock,
}

/// Message to handle transaction events for streaming
#[derive(Message)]
#[rtype(result = "Result<(), StreamError>")]
pub struct TransactionEventMessage {
    pub tx_hash: H256,
    pub transaction: Option<Transaction>,
}

/// Message to handle log events for streaming
#[derive(Message)]
#[rtype(result = "Result<(), StreamError>")]
pub struct LogEventMessage {
    pub log: EventLog,
    pub block_hash: BlockHash,
    pub tx_hash: H256,
}

/// Message to get connection status
#[derive(Message)]
#[rtype(result = "ConnectionStats")]
pub struct GetConnectionStatsMessage;

/// Message to get streaming statistics
#[derive(Message)]
#[rtype(result = "StreamingStats")]
pub struct GetStreamingStatsMessage;

/// Message to authenticate a connection
#[derive(Message)]
#[rtype(result = "Result<AuthResult, StreamError>")]
pub struct AuthenticateConnectionMessage {
    pub connection_id: String,
    pub credentials: AuthCredentials,
}

/// Message to handle ping/pong for connection health
#[derive(Message)]
#[rtype(result = "()")]
pub struct PingMessage {
    pub connection_id: String,
}

/// Message to handle custom client requests
#[derive(Message)]
#[rtype(result = "Result<serde_json::Value, StreamError>")]
pub struct ClientRequestMessage {
    pub connection_id: String,
    pub request_id: String,
    pub method: String,
    pub params: serde_json::Value,
}

/// A message to be streamed to clients
#[derive(Debug, Clone)]
pub struct StreamMessage {
    pub topic: String,
    pub event_type: String,
    pub data: serde_json::Value,
    pub timestamp: std::time::SystemTime,
    pub sequence_number: Option<u64>,
}

/// Subscription filters for topic data
#[derive(Debug, Clone)]
pub struct SubscriptionFilters {
    pub address_filters: Option<Vec<Address>>,
    pub topic_filters: Option<Vec<H256>>,
    pub from_block: Option<u64>,
    pub to_block: Option<u64>,
}

/// Authentication credentials
#[derive(Debug, Clone)]
pub enum AuthCredentials {
    Bearer { token: String },
    ApiKey { key: String },
    Signature { message: String, signature: Vec<u8> },
    None,
}

/// Authentication result
#[derive(Debug, Clone)]
pub struct AuthResult {
    pub authenticated: bool,
    pub user_id: Option<String>,
    pub permissions: Vec<Permission>,
    pub rate_limits: RateLimits,
}

/// User permissions
#[derive(Debug, Clone)]
pub enum Permission {
    ReadBlocks,
    ReadTransactions,
    ReadLogs,
    ReadState,
    Subscribe(String), // topic
    Admin,
}

/// Rate limiting configuration
#[derive(Debug, Clone)]
pub struct RateLimits {
    pub requests_per_minute: u32,
    pub bytes_per_minute: u64,
    pub subscriptions_limit: u32,
}

/// Connection statistics
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub active_connections: u32,
    pub total_connections: u64,
    pub authenticated_connections: u32,
    pub subscriptions_by_topic: std::collections::HashMap<String, u32>,
    pub data_sent_bytes: u64,
    pub messages_sent: u64,
}

/// Streaming statistics
#[derive(Debug, Clone)]
pub struct StreamingStats {
    pub connection_stats: ConnectionStats,
    pub topic_stats: std::collections::HashMap<String, TopicStats>,
    pub performance_metrics: PerformanceMetrics,
}

/// Statistics per topic
#[derive(Debug, Clone)]
pub struct TopicStats {
    pub topic: String,
    pub subscriber_count: u32,
    pub messages_sent: u64,
    pub bytes_sent: u64,
    pub last_message_time: Option<std::time::SystemTime>,
}

/// Performance metrics for streaming
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub average_latency_ms: f64,
    pub message_queue_size: u32,
    pub dropped_messages: u64,
    pub error_count: u64,
    pub uptime: std::time::Duration,
}

/// Event log for streaming
#[derive(Debug, Clone)]
pub struct EventLog {
    pub address: Address,
    pub topics: Vec<H256>,
    pub data: Vec<u8>,
    pub log_index: u32,
    pub removed: bool,
}

/// WebSocket frame types
#[derive(Debug, Clone)]
pub enum WebSocketFrame {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close(Option<CloseFrame>),
}

/// WebSocket close frame
#[derive(Debug, Clone)]
pub struct CloseFrame {
    pub code: u16,
    pub reason: String,
}

/// Stream event types
#[derive(Debug, Clone)]
pub enum StreamEventType {
    NewBlock,
    NewTransaction,
    NewLog,
    PendingTransaction,
    BlockReorg,
    StateChange,
    Custom(String),
}

/// Real-time block data for streaming
#[derive(Debug, Clone)]
pub struct StreamBlockData {
    pub hash: BlockHash,
    pub number: u64,
    pub parent_hash: BlockHash,
    pub timestamp: u64,
    pub transaction_count: u32,
    pub gas_used: u64,
    pub gas_limit: u64,
    pub base_fee: Option<U256>,
}

/// Real-time transaction data for streaming
#[derive(Debug, Clone)]
pub struct StreamTransactionData {
    pub hash: H256,
    pub from: Address,
    pub to: Option<Address>,
    pub value: U256,
    pub gas_limit: u64,
    pub gas_price: U256,
    pub status: TransactionStatus,
    pub block_hash: Option<BlockHash>,
    pub block_number: Option<u64>,
}

/// Transaction status for streaming
#[derive(Debug, Clone)]
pub enum TransactionStatus {
    Pending,
    Included,
    Failed { reason: String },
    Replaced { by: H256 },
}