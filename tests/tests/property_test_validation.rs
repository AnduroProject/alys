//! Validation tests for Phase 4: Property-Based Testing implementation
//!
//! These tests validate ALYS-002-17: Actor message ordering property tests
//! with sequence verification functionality.

use alys_test_framework::framework::generators::*;
use alys_test_framework::property_tests::*;
use proptest::prelude::*;
use std::time::SystemTime;

/// Test the property test framework components individually
#[cfg(test)]
mod validation_tests {
    use super::*;

    #[test]
    fn test_actor_message_generation() {
        let strategy = actor_message_strategy();
        let test_runner = &mut proptest::test_runner::TestRunner::default();
        
        // Generate a few messages to verify the strategy works
        for _ in 0..10 {
            let message = strategy.new_tree(test_runner).unwrap().current();
            
            // Verify message has all required fields
            assert!(!message.message_id.is_empty());
            assert!(!message.sender_id.is_empty());
            assert!(!message.receiver_id.is_empty());
            assert!(message.sequence_id > 0);
        }
    }

    #[test]
    fn test_ordered_message_sequence_generation() {
        let strategy = ordered_message_sequence_strategy();
        let test_runner = &mut proptest::test_runner::TestRunner::default();
        
        let messages = strategy.new_tree(test_runner).unwrap().current();
        
        // Verify sequence numbering is monotonic per sender
        let mut sender_sequences: std::collections::HashMap<String, Vec<u64>> = std::collections::HashMap::new();
        for msg in &messages {
            sender_sequences.entry(msg.sender_id.clone()).or_default()
                .push(msg.sequence_id);
        }
        
        for (sender_id, mut sequences) in sender_sequences {
            sequences.sort();
            for window in sequences.windows(2) {
                assert!(
                    window[1] > window[0],
                    "Non-monotonic sequence for sender {}: {} after {}", 
                    sender_id, window[1], window[0]
                );
            }
        }
    }

    #[test]
    fn test_mixed_priority_scenario_generation() {
        let strategy = mixed_priority_scenario_strategy();
        let test_runner = &mut proptest::test_runner::TestRunner::default();
        
        let scenario = strategy.new_tree(test_runner).unwrap().current();
        
        // Verify priority distribution
        let critical_count = scenario.messages.iter()
            .filter(|m| m.priority == MessagePriority::Critical).count();
        let high_count = scenario.messages.iter()
            .filter(|m| m.priority == MessagePriority::High).count();
        let normal_count = scenario.messages.iter()
            .filter(|m| m.priority == MessagePriority::Normal).count();
        let low_count = scenario.messages.iter()
            .filter(|m| m.priority == MessagePriority::Low).count();
        
        assert_eq!(critical_count + high_count + normal_count + low_count, scenario.messages.len());
    }

    #[tokio::test]
    async fn test_ordering_test_actor_basic_functionality() {
        let mut actor = OrderingTestActor::new("test_actor".to_string());
        
        let messages = vec![
            ActorMessage {
                message_id: "msg_1".to_string(),
                sender_id: "sender_a".to_string(),
                receiver_id: "test_actor".to_string(),
                message_type: ActorMessageType::Lifecycle(LifecycleMessage::Start),
                payload: vec![1, 2, 3],
                timestamp: SystemTime::now(),
                priority: MessagePriority::Normal,
                retry_count: 0,
                sequence_id: 1,
            },
            ActorMessage {
                message_id: "msg_2".to_string(),
                sender_id: "sender_a".to_string(),
                receiver_id: "test_actor".to_string(),
                message_type: ActorMessageType::Lifecycle(LifecycleMessage::Stop),
                payload: vec![4, 5, 6],
                timestamp: SystemTime::now(),
                priority: MessagePriority::High,
                retry_count: 0,
                sequence_id: 2,
            },
        ];

        let result = actor.process_messages_with_verification(messages).await.unwrap();
        
        // High priority message should be processed first
        assert_eq!(result.message_log.len(), 2);
        assert_eq!(result.message_log[0].priority, MessagePriority::High);
        assert_eq!(result.message_log[1].priority, MessagePriority::Normal);
        
        // No sequence violations expected
        assert!(result.sequence_violations.is_empty());
    }

    #[tokio::test]
    async fn test_sequence_violation_detection() {
        let mut actor = OrderingTestActor::new("test_actor".to_string());
        
        // Create messages with intentional sequence violation
        let messages = vec![
            ActorMessage {
                message_id: "msg_1".to_string(),
                sender_id: "sender_a".to_string(),
                receiver_id: "test_actor".to_string(),
                message_type: ActorMessageType::Lifecycle(LifecycleMessage::Start),
                payload: vec![1, 2, 3],
                timestamp: SystemTime::now(),
                priority: MessagePriority::Normal,
                retry_count: 0,
                sequence_id: 1,
            },
            ActorMessage {
                message_id: "msg_2".to_string(),
                sender_id: "sender_a".to_string(),
                receiver_id: "test_actor".to_string(),
                message_type: ActorMessageType::Lifecycle(LifecycleMessage::Stop),
                payload: vec![4, 5, 6],
                timestamp: SystemTime::now(),
                priority: MessagePriority::Normal,
                retry_count: 0,
                sequence_id: 1, // Same sequence ID - should trigger violation
            },
        ];

        let result = actor.process_messages_with_verification(messages).await.unwrap();
        
        // Should detect sequence violation
        assert!(!result.sequence_violations.is_empty());
        assert_eq!(result.sequence_violations[0].sender_id, "sender_a");
        assert_eq!(result.sequence_violations[0].actual_sequence, 1);
        assert_eq!(result.sequence_violations[0].expected_sequence, 2);
    }

    #[tokio::test]
    async fn test_throughput_measurement() {
        let mut actor = OrderingTestActor::new("throughput_test".to_string());
        
        // Generate 100 messages for throughput test
        let messages: Vec<_> = (0..100).map(|i| {
            ActorMessage {
                message_id: format!("msg_{}", i),
                sender_id: format!("sender_{}", i % 10),
                receiver_id: "throughput_test".to_string(),
                message_type: ActorMessageType::Lifecycle(LifecycleMessage::StatusQuery),
                payload: vec![i as u8],
                timestamp: SystemTime::now(),
                priority: MessagePriority::Normal,
                retry_count: 0,
                sequence_id: (i / 10) + 1, // 10 messages per sender
            }
        }).collect();

        let result = actor.process_messages_with_verification(messages).await.unwrap();
        
        // Verify throughput calculation
        assert_eq!(result.total_messages, 100);
        assert!(result.throughput > 0.0);
        assert!(result.total_duration.as_millis() > 0);
    }
}