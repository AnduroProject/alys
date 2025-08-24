//! Macros for common actor patterns in the Alys blockchain system
//!
//! This module provides convenience macros to reduce boilerplate when
//! implementing actors with standard patterns.

/// Generate a basic actor implementation with standard patterns
#[macro_export]
macro_rules! impl_alys_actor {
    (
        $actor:ident,
        config = $config:ty,
        state = $state:ty,
        message = $message:ty
    ) => {
        impl actix::Actor for $actor {
            type Context = actix::Context<Self>;
            
            fn started(&mut self, ctx: &mut Self::Context) {
                tracing::info!(
                    actor_type = stringify!($actor),
                    actor_id = %self.config().actor_id.as_ref().unwrap_or(&"unknown".to_string()),
                    "Actor started"
                );
                self.metrics_mut().record_actor_started();
                
                if let Err(e) = self.on_start(ctx) {
                    tracing::error!(
                        actor_type = stringify!($actor),
                        error = %e,
                        "Failed to start actor"
                    );
                    ctx.stop();
                }
            }
            
            fn stopped(&mut self, _ctx: &mut Self::Context) {
                tracing::info!(
                    actor_type = stringify!($actor),
                    "Actor stopped"
                );
                self.metrics_mut().record_actor_stopped();
            }
        }
        
        #[actix::prelude::async_trait]
        impl $crate::actor::AlysActor for $actor {
            type Config = $config;
            type Error = $crate::error::ActorError;
            type Message = $message;
            type State = $state;
            
            fn actor_type(&self) -> String {
                stringify!($actor).to_string()
            }
        }
    };
}

/// Generate blockchain-aware actor implementation
#[macro_export]
macro_rules! impl_blockchain_actor {
    (
        $actor:ident,
        config = $config:ty,
        state = $state:ty,
        message = $message:ty,
        priority = $priority:expr
    ) => {
        impl_alys_actor!($actor, config = $config, state = $state, message = $message);
        
        #[actix::prelude::async_trait]
        impl $crate::blockchain::BlockchainAwareActor for $actor {
            fn blockchain_priority(&self) -> $crate::blockchain::BlockchainActorPriority {
                $priority
            }
            
            fn is_consensus_critical(&self) -> bool {
                matches!($priority, $crate::blockchain::BlockchainActorPriority::Consensus)
            }
        }
    };
}

/// Generate message handler with error handling and metrics
#[macro_export]
macro_rules! impl_message_handler {
    ($actor:ident, $message:ty => $result:ty, $handler:ident) => {
        impl actix::Handler<$message> for $actor {
            type Result = actix::ResponseActFuture<Self, $result>;
            
            fn handle(&mut self, msg: $message, ctx: &mut Self::Context) -> Self::Result {
                let start_time = std::time::Instant::now();
                let message_id = uuid::Uuid::new_v4();
                
                tracing::debug!(
                    actor_type = stringify!($actor),
                    message_type = stringify!($message),
                    message_id = %message_id,
                    "Handling message"
                );
                
                self.metrics_mut().record_message_received(stringify!($message));
                
                let fut = async move {
                    let result = self.$handler(msg).await;
                    
                    let duration = start_time.elapsed();
                    match &result {
                        Ok(_) => {
                            self.metrics_mut().record_message_processed(
                                stringify!($message), 
                                duration
                            );
                            tracing::debug!(
                                actor_type = stringify!($actor),
                                message_type = stringify!($message),
                                message_id = %message_id,
                                duration_ms = duration.as_millis(),
                                "Message handled successfully"
                            );
                        }
                        Err(e) => {
                            self.metrics_mut().record_message_failed(stringify!($message));
                            tracing::error!(
                                actor_type = stringify!($actor),
                                message_type = stringify!($message),
                                message_id = %message_id,
                                error = %e,
                                duration_ms = duration.as_millis(),
                                "Message handling failed"
                            );
                        }
                    }
                    
                    result
                };
                
                Box::pin(fut.into_actor(self))
            }
        }
    };
}

/// Generate supervised actor factory
#[macro_export]
macro_rules! impl_supervised_factory {
    ($actor:ident, $config:ty) => {
        pub struct [<$actor Factory>] {
            config: $config,
        }
        
        impl [<$actor Factory>] {
            pub fn new(config: $config) -> Self {
                Self { config }
            }
        }
        
        impl $crate::supervisor::ActorFactory<$actor> for [<$actor Factory>] {
            fn create(&self) -> $actor {
                $actor::new(self.config.clone()).expect("Failed to create actor")
            }
            
            fn config(&self) -> $crate::supervisor::SupervisedActorConfig {
                $crate::supervisor::SupervisedActorConfig {
                    restart_strategy: $crate::supervisor::RestartStrategy::default(),
                    max_restarts: Some(10),
                    restart_window: std::time::Duration::from_secs(60),
                    escalation_strategy: $crate::supervisor::EscalationStrategy::EscalateToParent,
                }
            }
        }
    };
}

/// Generate health check implementation for an actor
#[macro_export]
macro_rules! impl_health_check {
    ($actor:ident) => {
        impl actix::Handler<$crate::actor::HealthCheck> for $actor {
            type Result = actix::ResponseActFuture<Self, $crate::error::ActorResult<bool>>;
            
            fn handle(&mut self, _msg: $crate::actor::HealthCheck, _ctx: &mut Self::Context) -> Self::Result {
                Box::pin(async move {
                    self.health_check().await.map_err(|e| e.into())
                }.into_actor(self))
            }
        }
    };
}

/// Generate configuration update handler
#[macro_export]
macro_rules! impl_config_update {
    ($actor:ident, $config:ty) => {
        impl actix::Handler<$crate::actor::ConfigUpdate<$config>> for $actor {
            type Result = actix::ResponseActFuture<Self, $crate::error::ActorResult<()>>;
            
            fn handle(&mut self, msg: $crate::actor::ConfigUpdate<$config>, _ctx: &mut Self::Context) -> Self::Result {
                Box::pin(async move {
                    self.on_config_update(msg.config).await
                }.into_actor(self))
            }
        }
    };
}

/// Generate shutdown handler
#[macro_export]
macro_rules! impl_shutdown {
    ($actor:ident) => {
        impl actix::Handler<$crate::actor::Shutdown> for $actor {
            type Result = actix::ResponseActFuture<Self, $crate::error::ActorResult<()>>;
            
            fn handle(&mut self, msg: $crate::actor::Shutdown, ctx: &mut Self::Context) -> Self::Result {
                Box::pin(async move {
                    tracing::info!(
                        actor_type = stringify!($actor),
                        "Shutdown requested"
                    );
                    
                    let result = self.on_shutdown(msg.timeout).await;
                    ctx.stop();
                    result
                }.into_actor(self))
            }
        }
    };
}

/// Generate all standard handlers for an actor
#[macro_export]
macro_rules! impl_standard_handlers {
    ($actor:ident, $config:ty) => {
        impl_health_check!($actor);
        impl_config_update!($actor, $config);
        impl_shutdown!($actor);
    };
}

/// Generate metrics collection for an actor
#[macro_export]
macro_rules! impl_metrics_collection {
    ($actor:ident) => {
        impl $actor {
            /// Export actor metrics as JSON
            pub async fn export_metrics(&self) -> serde_json::Value {
                let snapshot = self.metrics().snapshot();
                serde_json::to_value(snapshot).unwrap_or_default()
            }
            
            /// Get current actor statistics
            pub fn get_stats(&self) -> $crate::metrics::ActorStats {
                self.metrics().get_stats()
            }
        }
    };
}

/// Generate blockchain event subscription for an actor
#[macro_export]
macro_rules! impl_blockchain_events {
    ($actor:ident) => {
        impl actix::Handler<$crate::blockchain::BlockchainEvent> for $actor {
            type Result = actix::ResponseActFuture<Self, $crate::error::ActorResult<()>>;
            
            fn handle(&mut self, msg: $crate::blockchain::BlockchainEvent, _ctx: &mut Self::Context) -> Self::Result {
                Box::pin(async move {
                    self.handle_blockchain_event(msg).await
                }.into_actor(self))
            }
        }
        
        impl actix::Handler<$crate::blockchain::CheckBlockchainReadiness> for $actor {
            type Result = actix::ResponseActFuture<Self, $crate::error::ActorResult<$crate::blockchain::BlockchainReadiness>>;
            
            fn handle(&mut self, _msg: $crate::blockchain::CheckBlockchainReadiness, _ctx: &mut Self::Context) -> Self::Result {
                Box::pin(async move {
                    self.validate_blockchain_readiness().await
                }.into_actor(self))
            }
        }
    };
}