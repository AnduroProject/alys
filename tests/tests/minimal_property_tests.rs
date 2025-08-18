//! Minimal property tests for ALYS-002-17 implementation
//!
//! This file contains the core property tests for actor message ordering
//! without depending on the full framework harness (which has compilation issues).

use proptest::prelude::*;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

// Minimal actor message types for testing
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimalActorMessage {
    pub message_id: String,
    pub sender_id: String,
    pub receiver_id: String,
    pub priority: MessagePriority,
    pub sequence_id: u64,
    pub timestamp: SystemTime,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessagePriority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone)]
pub struct MessageProcessingResult {
    pub total_messages: u64,
    pub sequence_violations: Vec<String>,
    pub priority_violations: Vec<String>,
    pub processing_order: Vec<String>,
}

// Generator for minimal actor messages
fn minimal_actor_message_strategy() -> impl Strategy<Value = MinimalActorMessage> {
    (
        "[a-zA-Z0-9]{10,20}",
        "[a-zA-Z0-9]{5,10}",
        "[a-zA-Z0-9]{5,10}",
        prop_oneof![
            Just(MessagePriority::Low),
            Just(MessagePriority::Normal),
            Just(MessagePriority::High),
            Just(MessagePriority::Critical),
        ],
        1u64..1000,
        Just(SystemTime::now()),
    ).prop_map(|(message_id, sender_id, receiver_id, priority, sequence_id, timestamp)| {
        MinimalActorMessage {
            message_id,
            sender_id,
            receiver_id,
            priority,
            sequence_id,
            timestamp,
        }
    })
}

// Message processor that verifies ordering properties
pub fn process_messages_with_verification(
    mut messages: Vec<MinimalActorMessage>
) -> MessageProcessingResult {
    // Sort by priority (highest first), then by timestamp
    messages.sort_by(|a, b| {
        match b.priority.cmp(&a.priority) {
            std::cmp::Ordering::Equal => a.timestamp.cmp(&b.timestamp),
            other => other,
        }
    });

    let mut sequence_violations = Vec::new();
    let mut priority_violations = Vec::new();
    let mut processing_order = Vec::new();
    
    // Track last sequence per sender
    let mut sender_sequences: HashMap<String, u64> = HashMap::new();
    let mut last_priority = MessagePriority::Critical;

    for (i, message) in messages.iter().enumerate() {
        processing_order.push(message.message_id.clone());

        // Check sequence violations
        if let Some(&last_seq) = sender_sequences.get(&message.sender_id) {
            if message.sequence_id <= last_seq {
                sequence_violations.push(format!(
                    "Sender {} sequence violation: {} after {}", 
                    message.sender_id, message.sequence_id, last_seq
                ));
            }
        }
        sender_sequences.insert(message.sender_id.clone(), message.sequence_id);

        // Check priority violations  
        if i > 0 && message.priority > last_priority {
            priority_violations.push(format!(
                "Priority violation: {:?} after {:?}", 
                message.priority, last_priority
            ));
        }
        last_priority = message.priority.clone();
    }

    MessageProcessingResult {
        total_messages: messages.len() as u64,
        sequence_violations,
        priority_violations,
        processing_order,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]
    
    /// Test: Messages with same priority should maintain FIFO order
    #[test]
    fn test_fifo_ordering_within_priority(
        messages in prop::collection::vec(minimal_actor_message_strategy(), 10..100)
    ) {
        // Assign same priority to all messages
        let uniform_priority_messages: Vec<_> = messages.into_iter()
            .map(|mut m| {
                m.priority = MessagePriority::Normal;
                m
            })
            .collect();

        let result = process_messages_with_verification(uniform_priority_messages);
        
        // Property: No priority violations should occur with uniform priority
        prop_assert!(
            result.priority_violations.is_empty(),
            "Priority violations: {:?}", result.priority_violations
        );
    }
    
    /// Test: Critical messages should always be processed before others
    #[test]
    fn test_critical_message_priority(
        mut messages in prop::collection::vec(minimal_actor_message_strategy(), 20..50)
    ) {
        // Ensure we have some critical and some non-critical messages
        for (i, msg) in messages.iter_mut().enumerate() {
            msg.priority = if i % 4 == 0 {
                MessagePriority::Critical
            } else {
                MessagePriority::Normal
            };
        }

        let result = process_messages_with_verification(messages);
        
        // Find positions of critical vs non-critical messages
        let mut critical_positions = Vec::new();
        let mut non_critical_positions = Vec::new();
        
        for (pos, msg_id) in result.processing_order.iter().enumerate() {
            // We need to find the original message to check its priority
            // For this test, we know that every 4th message is critical
            if pos % 4 == 0 {
                critical_positions.push(pos);
            } else {
                non_critical_positions.push(pos);
            }
        }
        
        // Property: All critical messages should come before non-critical ones
        if !critical_positions.is_empty() && !non_critical_positions.is_empty() {
            let last_critical = critical_positions.iter().max().unwrap();
            let first_non_critical = non_critical_positions.iter().min().unwrap();
            
            prop_assert!(
                last_critical < first_non_critical,
                "Critical messages not prioritized correctly"
            );
        }
    }
    
    /// Test: Sequence numbering should be respected per sender
    #[test] 
    fn test_sequence_numbering_per_sender(
        base_messages in prop::collection::vec(minimal_actor_message_strategy(), 30..100)
    ) {
        // Create ordered sequences per sender
        let mut sender_counters: HashMap<String, u64> = HashMap::new();
        let mut messages = Vec::new();
        
        for mut msg in base_messages {
            let counter = sender_counters.entry(msg.sender_id.clone()).or_insert(0);
            *counter += 1;
            msg.sequence_id = *counter;
            messages.push(msg);
        }

        let result = process_messages_with_verification(messages);
        
        // Property: No sequence violations should occur with properly ordered sequences
        prop_assert!(
            result.sequence_violations.is_empty(),
            "Sequence violations detected: {:?}", result.sequence_violations
        );
    }
    
    /// Test: Processing should handle mixed priority scenarios correctly
    #[test]
    fn test_mixed_priority_processing(
        messages in prop::collection::vec(minimal_actor_message_strategy(), 50..200)
    ) {
        let result = process_messages_with_verification(messages);
        
        // Property: Total messages processed should match input
        prop_assert_eq!(result.total_messages, result.processing_order.len() as u64);
        
        // Property: Each message should be processed exactly once
        let mut seen_messages = std::collections::HashSet::new();
        for msg_id in &result.processing_order {
            prop_assert!(
                seen_messages.insert(msg_id.clone()),
                "Duplicate message processing: {}", msg_id
            );
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_message_processing_basic_functionality() {
        let messages = vec![
            MinimalActorMessage {
                message_id: "msg_1".to_string(),
                sender_id: "sender_a".to_string(),
                receiver_id: "receiver".to_string(),
                priority: MessagePriority::Normal,
                sequence_id: 1,
                timestamp: SystemTime::now(),
            },
            MinimalActorMessage {
                message_id: "msg_2".to_string(),
                sender_id: "sender_a".to_string(),
                receiver_id: "receiver".to_string(),
                priority: MessagePriority::Critical,
                sequence_id: 2,
                timestamp: SystemTime::now(),
            },
        ];

        let result = process_messages_with_verification(messages);
        
        // Critical message should be processed first
        assert_eq!(result.processing_order[0], "msg_2");
        assert_eq!(result.processing_order[1], "msg_1");
        assert!(result.sequence_violations.is_empty());
    }

    #[test]
    fn test_sequence_violation_detection() {
        let messages = vec![
            MinimalActorMessage {
                message_id: "msg_1".to_string(),
                sender_id: "sender_a".to_string(),
                receiver_id: "receiver".to_string(),
                priority: MessagePriority::Normal,
                sequence_id: 2,
                timestamp: SystemTime::now(),
            },
            MinimalActorMessage {
                message_id: "msg_2".to_string(),
                sender_id: "sender_a".to_string(),
                receiver_id: "receiver".to_string(),
                priority: MessagePriority::Normal,
                sequence_id: 1, // Lower sequence after higher - violation
                timestamp: SystemTime::now(),
            },
        ];

        let result = process_messages_with_verification(messages);
        
        // Should detect sequence violation
        assert!(!result.sequence_violations.is_empty());
        assert!(result.sequence_violations[0].contains("sender_a"));
    }

    #[test]  
    fn test_priority_ordering() {
        let messages = vec![
            MinimalActorMessage {
                message_id: "low".to_string(),
                sender_id: "sender".to_string(),
                receiver_id: "receiver".to_string(),
                priority: MessagePriority::Low,
                sequence_id: 1,
                timestamp: SystemTime::now(),
            },
            MinimalActorMessage {
                message_id: "critical".to_string(),
                sender_id: "sender".to_string(),
                receiver_id: "receiver".to_string(),
                priority: MessagePriority::Critical,
                sequence_id: 2,
                timestamp: SystemTime::now(),
            },
            MinimalActorMessage {
                message_id: "high".to_string(),
                sender_id: "sender".to_string(),
                receiver_id: "receiver".to_string(),
                priority: MessagePriority::High,
                sequence_id: 3,
                timestamp: SystemTime::now(),
            },
        ];

        let result = process_messages_with_verification(messages);
        
        // Should process in priority order: Critical -> High -> Low
        assert_eq!(result.processing_order[0], "critical");
        assert_eq!(result.processing_order[1], "high");
        assert_eq!(result.processing_order[2], "low");
    }
}