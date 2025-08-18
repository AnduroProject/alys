//! Property-Based Tests for Alys V2 Testing Framework
//!
//! This module contains property tests for validating critical system behaviors
//! using the PropTest framework. Tests verify invariants across randomized inputs
//! to ensure system reliability under diverse conditions.

use proptest::prelude::*;
use std::time::{Duration, SystemTime};
use std::collections::{HashMap, VecDeque};
use crate::framework::generators::*;
use crate::framework::TestResult;
use actix::prelude::*;

// ALYS-002-17: Actor Message Ordering Property Tests with Sequence Verification

/// Test actor for message ordering verification
#[derive(Debug, Clone)]
pub struct OrderingTestActor {
    pub actor_id: String,
    pub message_log: Vec<ProcessedMessage>,
    pub sequence_counter: u64,
    pub mailbox: VecDeque<ActorMessage>,
    pub processing_delays: HashMap<String, Duration>,
}

/// Processed message with ordering information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessedMessage {
    pub message_id: String,
    pub sender_id: String,
    pub message_type: ActorMessageType,
    pub priority: MessagePriority,
    pub sequence_number: u64,
    pub processing_order: u64,
    pub received_at: SystemTime,
    pub processed_at: SystemTime,
}

impl Actor for OrderingTestActor {
    type Context = Context<Self>;
}

impl OrderingTestActor {
    pub fn new(actor_id: String) -> Self {
        Self {
            actor_id,
            message_log: Vec::new(),
            sequence_counter: 0,
            mailbox: VecDeque::new(),
            processing_delays: HashMap::new(),
        }
    }

    /// Process a batch of messages and verify ordering properties
    pub async fn process_messages_with_verification(
        &mut self, 
        mut messages: Vec<ActorMessage>
    ) -> Result<MessageProcessingResult, String> {
        // Sort messages by priority (Critical > High > Normal > Low) and then by timestamp
        messages.sort_by(|a, b| {
            match b.priority.cmp(&a.priority) {
                std::cmp::Ordering::Equal => a.timestamp.cmp(&b.timestamp),
                other => other,
            }
        });

        let start_time = SystemTime::now();
        let mut processing_order = 0;
        let mut sequence_violations = Vec::new();
        let mut priority_violations = Vec::new();

        for message in messages {
            let received_at = SystemTime::now();
            
            // Verify sequence number is monotonically increasing within same sender
            if let Some(last_msg) = self.message_log.iter()
                .filter(|m| m.sender_id == message.sender_id)
                .last() {
                if message.sequence_id <= last_msg.sequence_number {
                    sequence_violations.push(SequenceViolation {
                        sender_id: message.sender_id.clone(),
                        expected_sequence: last_msg.sequence_number + 1,
                        actual_sequence: message.sequence_id,
                        message_id: message.message_id.clone(),
                    });
                }
            }

            // Verify priority ordering
            if let Some(last_processed) = self.message_log.last() {
                if message.priority < last_processed.priority {
                    priority_violations.push(PriorityViolation {
                        previous_message_id: last_processed.message_id.clone(),
                        previous_priority: last_processed.priority.clone(),
                        current_message_id: message.message_id.clone(),
                        current_priority: message.priority.clone(),
                    });
                }
            }

            // Simulate processing delay based on message type
            let processing_delay = self.get_processing_delay(&message.message_type);
            if processing_delay > Duration::ZERO {
                tokio::time::sleep(processing_delay).await;
            }

            let processed_at = SystemTime::now();
            
            // Record processed message
            let processed_msg = ProcessedMessage {
                message_id: message.message_id.clone(),
                sender_id: message.sender_id.clone(),
                message_type: message.message_type.clone(),
                priority: message.priority.clone(),
                sequence_number: message.sequence_id,
                processing_order,
                received_at,
                processed_at,
            };

            self.message_log.push(processed_msg);
            processing_order += 1;
        }

        let total_duration = start_time.elapsed().unwrap_or_default();

        Ok(MessageProcessingResult {
            total_messages: processing_order,
            total_duration,
            sequence_violations,
            priority_violations,
            throughput: processing_order as f64 / total_duration.as_secs_f64(),
            message_log: self.message_log.clone(),
        })
    }

    fn get_processing_delay(&self, message_type: &ActorMessageType) -> Duration {
        match message_type {
            ActorMessageType::Lifecycle(_) => Duration::from_millis(1),
            ActorMessageType::Sync(_) => Duration::from_millis(5),
            ActorMessageType::Network(_) => Duration::from_millis(2),
            ActorMessageType::Mining(_) => Duration::from_millis(10),
            ActorMessageType::Governance(_) => Duration::from_millis(15),
        }
    }
}

/// Result of message processing with ordering verification
#[derive(Debug, Clone)]
pub struct MessageProcessingResult {
    pub total_messages: u64,
    pub total_duration: Duration,
    pub sequence_violations: Vec<SequenceViolation>,
    pub priority_violations: Vec<PriorityViolation>,
    pub throughput: f64,
    pub message_log: Vec<ProcessedMessage>,
}

#[derive(Debug, Clone)]
pub struct SequenceViolation {
    pub sender_id: String,
    pub expected_sequence: u64,
    pub actual_sequence: u64,
    pub message_id: String,
}

#[derive(Debug, Clone)]
pub struct PriorityViolation {
    pub previous_message_id: String,
    pub previous_priority: MessagePriority,
    pub current_message_id: String,
    pub current_priority: MessagePriority,
}

/// Property test strategies for message ordering scenarios
pub fn ordered_message_sequence_strategy() -> impl Strategy<Value = Vec<ActorMessage>> {
    prop::collection::vec(actor_message_strategy(), 10..100)
        .prop_map(|mut messages| {
            // Ensure monotonic sequence IDs per sender
            let mut sender_sequences: HashMap<String, u64> = HashMap::new();
            for msg in &mut messages {
                let next_seq = sender_sequences.get(&msg.sender_id).unwrap_or(&0) + 1;
                sender_sequences.insert(msg.sender_id.clone(), next_seq);
                msg.sequence_id = next_seq;
            }
            messages
        })
}

pub fn mixed_priority_scenario_strategy() -> impl Strategy<Value = MixedPriorityScenario> {
    (
        prop::collection::vec(actor_message_strategy(), 50..200),
        0.0f64..1.0, // critical_ratio
        0.0f64..0.5, // high_ratio  
        0.2f64..0.6, // normal_ratio (remainder is low)
    ).prop_map(|(mut messages, critical_ratio, high_ratio, normal_ratio)| {
        let total = messages.len();
        let critical_count = (total as f64 * critical_ratio) as usize;
        let high_count = (total as f64 * high_ratio) as usize;
        let normal_count = (total as f64 * normal_ratio) as usize;
        
        // Assign priorities
        for (i, msg) in messages.iter_mut().enumerate() {
            msg.priority = if i < critical_count {
                MessagePriority::Critical
            } else if i < critical_count + high_count {
                MessagePriority::High
            } else if i < critical_count + high_count + normal_count {
                MessagePriority::Normal
            } else {
                MessagePriority::Low
            };
        }

        MixedPriorityScenario { messages }
    })
}

#[derive(Debug, Clone)]
pub struct MixedPriorityScenario {
    pub messages: Vec<ActorMessage>,
}

// Property Tests Implementation

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]
    
    /// Test: Message sequence ordering must be preserved within same sender
    #[test]
    fn test_message_sequence_ordering_preservation(
        messages in ordered_message_sequence_strategy()
    ) {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let mut actor = OrderingTestActor::new("test_actor".to_string());
            
            // Group messages by sender to verify ordering
            let mut sender_groups: HashMap<String, Vec<&ActorMessage>> = HashMap::new();
            for msg in &messages {
                sender_groups.entry(msg.sender_id.clone()).or_default().push(msg);
            }
            
            let result = actor.process_messages_with_verification(messages).await
                .expect("Message processing should succeed");
            
            // Property: No sequence violations should occur
            assert!(
                result.sequence_violations.is_empty(),
                "Sequence violations detected: {:?}", result.sequence_violations
            );
            
            // Property: Messages from same sender should maintain sequence order
            for (sender_id, sender_messages) in sender_groups {
                let processed_msgs: Vec<_> = result.message_log.iter()
                    .filter(|m| m.sender_id == sender_id)
                    .collect();
                
                // Verify sequence numbers are monotonically increasing
                for window in processed_msgs.windows(2) {
                    assert!(
                        window[1].sequence_number > window[0].sequence_number,
                        "Sequence numbers not monotonic for sender {}: {} -> {}",
                        sender_id, window[0].sequence_number, window[1].sequence_number
                    );
                }
            }
        });
    }
    
    /// Test: Priority-based message ordering must be respected
    #[test]
    fn test_priority_based_message_ordering(
        scenario in mixed_priority_scenario_strategy()
    ) {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let mut actor = OrderingTestActor::new("priority_test_actor".to_string());
            
            let result = actor.process_messages_with_verification(scenario.messages).await
                .expect("Priority-based processing should succeed");
            
            // Property: Critical messages should be processed before all others
            let critical_msgs: Vec<_> = result.message_log.iter()
                .filter(|m| m.priority == MessagePriority::Critical)
                .collect();
            let non_critical_msgs: Vec<_> = result.message_log.iter()
                .filter(|m| m.priority != MessagePriority::Critical)
                .collect();
            
            if !critical_msgs.is_empty() && !non_critical_msgs.is_empty() {
                let last_critical_order = critical_msgs.iter()
                    .map(|m| m.processing_order)
                    .max().unwrap();
                let first_non_critical_order = non_critical_msgs.iter()
                    .map(|m| m.processing_order)
                    .min().unwrap();
                
                assert!(
                    last_critical_order < first_non_critical_order,
                    "Critical messages should be processed before non-critical messages"
                );
            }
            
            // Property: Within same priority, FIFO ordering should be maintained
            let priority_groups = [
                MessagePriority::Critical,
                MessagePriority::High, 
                MessagePriority::Normal,
                MessagePriority::Low,
            ];
            
            for priority in priority_groups {
                let priority_msgs: Vec<_> = result.message_log.iter()
                    .filter(|m| m.priority == priority)
                    .collect();
                
                // Within same priority, received_at timestamps should be in order
                for window in priority_msgs.windows(2) {
                    assert!(
                        window[0].received_at <= window[1].received_at,
                        "FIFO ordering violated within {:?} priority messages", priority
                    );
                }
            }
        });
    }
    
    /// Test: Message throughput should maintain minimum performance thresholds
    #[test] 
    fn test_message_processing_throughput(
        messages in prop::collection::vec(actor_message_strategy(), 100..1000)
    ) {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let mut actor = OrderingTestActor::new("throughput_test_actor".to_string());
            
            let result = actor.process_messages_with_verification(messages).await
                .expect("Throughput test should succeed");
            
            // Property: Minimum throughput threshold (messages per second)
            let min_throughput = 100.0; // 100 messages/second minimum
            assert!(
                result.throughput >= min_throughput,
                "Throughput {} msg/s below minimum {} msg/s", 
                result.throughput, min_throughput
            );
            
            // Property: Processing should complete within reasonable time bounds
            let max_duration = Duration::from_secs(30);
            assert!(
                result.total_duration <= max_duration,
                "Processing duration {:?} exceeds maximum {:?}",
                result.total_duration, max_duration
            );
        });
    }
    
    /// Test: Actor state consistency during concurrent message processing  
    #[test]
    fn test_actor_state_consistency_under_load(
        actor_scenario in actor_system_scenario_strategy()
    ) {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let mut actors: HashMap<String, OrderingTestActor> = HashMap::new();
            
            // Create actors for scenario
            for actor_id in &actor_scenario.actor_ids {
                actors.insert(actor_id.clone(), OrderingTestActor::new(actor_id.clone()));
            }
            
            // Distribute messages to actors
            for message in actor_scenario.messages {
                if let Some(actor) = actors.get_mut(&message.receiver_id) {
                    let result = actor.process_messages_with_verification(vec![message]).await
                        .expect("Single message processing should succeed");
                    
                    // Property: No sequence violations during individual processing
                    assert!(
                        result.sequence_violations.is_empty(),
                        "Sequence violations in actor {}: {:?}", 
                        actor.actor_id, result.sequence_violations
                    );
                }
            }
            
            // Property: All actors should maintain consistent state
            for (actor_id, actor) in &actors {
                // Verify message log integrity
                let mut prev_sequence_per_sender: HashMap<String, u64> = HashMap::new();
                
                for msg in &actor.message_log {
                    if let Some(&prev_seq) = prev_sequence_per_sender.get(&msg.sender_id) {
                        assert!(
                            msg.sequence_number > prev_seq,
                            "Actor {} has sequence violation: sender {} went from {} to {}",
                            actor_id, msg.sender_id, prev_seq, msg.sequence_number
                        );
                    }
                    prev_sequence_per_sender.insert(msg.sender_id.clone(), msg.sequence_number);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framework::generators::*;
    
    /// Integration test for property test framework
    #[tokio::test]
    async fn test_actor_message_ordering_framework() {
        let messages = vec![
            ActorMessage {
                message_id: "msg_1".to_string(),
                sender_id: "sender_a".to_string(), 
                receiver_id: "receiver_1".to_string(),
                message_type: ActorMessageType::Lifecycle(LifecycleMessage::Start),
                payload: vec![1, 2, 3],
                timestamp: SystemTime::now(),
                priority: MessagePriority::High,
                retry_count: 0,
                sequence_id: 1,
            },
            ActorMessage {
                message_id: "msg_2".to_string(),
                sender_id: "sender_a".to_string(),
                receiver_id: "receiver_1".to_string(), 
                message_type: ActorMessageType::Lifecycle(LifecycleMessage::Stop),
                payload: vec![4, 5, 6],
                timestamp: SystemTime::now(),
                priority: MessagePriority::Critical,
                retry_count: 0,
                sequence_id: 2,
            },
        ];
        
        let mut actor = OrderingTestActor::new("test".to_string());
        let result = actor.process_messages_with_verification(messages).await.unwrap();
        
        // Critical message should be processed first despite higher sequence number
        assert_eq!(result.message_log.len(), 2);
        assert_eq!(result.message_log[0].priority, MessagePriority::Critical);
        assert_eq!(result.message_log[1].priority, MessagePriority::High);
        assert!(result.sequence_violations.is_empty());
    }
    
    /// Test helper function for generating realistic message sequences
    #[test]
    fn test_ordered_message_sequence_generation() {
        let strategy = ordered_message_sequence_strategy();
        let messages = strategy.new_tree(&mut proptest::test_runner::TestRunner::default())
            .unwrap()
            .current();
        
        assert!(!messages.is_empty());
        
        // Verify sequence numbering is correct per sender
        let mut sender_sequences: HashMap<String, Vec<u64>> = HashMap::new();
        for msg in &messages {
            sender_sequences.entry(msg.sender_id.clone()).or_default()
                .push(msg.sequence_id);
        }
        
        for (sender_id, sequences) in sender_sequences {
            // Should be monotonically increasing
            let mut prev = 0;
            for &seq in &sequences {
                assert!(seq > prev, "Non-monotonic sequence for sender {}: {} after {}", 
                        sender_id, seq, prev);
                prev = seq;
            }
        }
    }
}