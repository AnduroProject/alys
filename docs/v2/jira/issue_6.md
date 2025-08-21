# ALYS-006: Implement Actor System Supervisor

## Issue Type
Task

## Priority
Critical

## Sprint
Migration Sprint 2

## Component
Core Architecture

## Labels
`migration`, `phase-1`, `actor-system`, `core`, `supervisor`

## Description

Implement the root actor supervisor that will manage the lifecycle of all actors in the system. This includes supervision strategies, restart policies, error recovery, and the foundational message-passing infrastructure that will replace the current Arc<RwLock<>> pattern.

## Acceptance Criteria

## Detailed Implementation Subtasks (26 tasks across 6 phases)

### Phase 1: Actor System Foundation (5 tasks)
- [X] **ALYS-006-01**: Design `ActorSystemConfig` with supervision settings, mailbox capacity, restart strategies, and metrics
- [X] **ALYS-006-02**: Implement `RestartStrategy` enum with Always, Never, ExponentialBackoff, and FixedDelay variants
- [X] **ALYS-006-03**: Create `RootSupervisor` structure with system management, configuration, and supervised actor tracking
- [X] **ALYS-006-04**: Implement actor system startup with arbiter creation, metrics initialization, and health monitoring
- [X] **ALYS-006-05**: Add system-wide constants and utility functions for backoff calculations and timing

### Phase 2: Supervision & Restart Logic (6 tasks)
- [X] **ALYS-006-06**: Implement `spawn_supervised` with actor factory pattern, registry integration, and mailbox configuration
- [X] **ALYS-006-07**: Create actor failure handling with error classification, restart counting, and metrics tracking
- [X] **ALYS-006-08**: Implement exponential backoff restart with configurable parameters, delay calculation, and max attempts
- [X] **ALYS-006-09**: Add fixed delay restart strategy with timing controls and failure counting
- [X] **ALYS-006-10**: Create restart attempt tracking with timestamps, success rates, and failure patterns
- [X] **ALYS-006-11**: Implement supervisor escalation for repeated failures and cascade prevention

### Phase 3: Actor Registry & Discovery (4 tasks)
- [X] **ALYS-006-12**: Implement `ActorRegistry` with name-based and type-based actor lookup capabilities
- [X] **ALYS-006-13**: Create actor registration system with unique name enforcement, type indexing, and lifecycle tracking
- [X] **ALYS-006-14**: Add actor discovery methods with type-safe address retrieval and batch operations
- [X] **ALYS-006-15**: Implement actor unregistration with cleanup, index maintenance, and orphan prevention

### Phase 4: Legacy Integration & Adapters (5 tasks) - ✅ **COMPLETE** (2024-01-20)
- [X] **ALYS-006-16**: Design `LegacyAdapter` pattern for gradual migration from `Arc<RwLock<T>>` to actor model - ✅ COMPLETE
- [X] **ALYS-006-17**: Implement `ChainAdapter` with feature flag integration and dual-path execution - ✅ COMPLETE
- [X] **ALYS-006-18**: Create `EngineAdapter` for EVM execution layer transition with backward compatibility - ✅ COMPLETE
- [X] **ALYS-006-19**: Add adapter testing framework with feature flag switching and performance comparison - ✅ COMPLETE
- [X] **ALYS-006-20**: Implement adapter metrics collection with latency comparison and migration progress tracking - ✅ COMPLETE

### Phase 5: Health Monitoring & Shutdown (4 tasks)
- [X] **ALYS-006-21**: Implement `HealthMonitor` actor with periodic health checks, failure detection, and recovery triggering
- [X] **ALYS-006-22**: Create actor health check protocol with ping/pong messaging and response time tracking
- [X] **ALYS-006-23**: Implement graceful shutdown with timeout handling, actor coordination, and cleanup procedures
- [X] **ALYS-006-24**: Add shutdown monitoring with progress tracking, forced termination, and resource cleanup

### Phase 6: Testing & Performance (2 tasks)
- [X] **ALYS-006-25**: Create comprehensive test suite with supervision testing, restart scenarios, and failure simulation
- [X] **ALYS-006-26**: Implement performance benchmarks with message throughput, latency measurement, and regression detection

## Original Acceptance Criteria
- [ ] Actor supervisor implemented with supervision tree
- [ ] Restart strategies configurable per actor
- [ ] Message routing infrastructure operational
- [ ] Actor registry for discovery and communication
- [ ] Mailbox overflow handling implemented
- [ ] Metrics collection for actor system
- [ ] Graceful shutdown mechanism
- [ ] No performance regression vs current system
- [ ] Integration with existing code via adapters

## Technical Details

### Implementation Steps

1. **Create Actor System Foundation**
```rust
// src/actors/mod.rs

use actix::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub mod supervisor;
pub mod registry;
pub mod messages;
pub mod adapters;

/// Root actor system configuration
#[derive(Debug, Clone)]
pub struct ActorSystemConfig {
    pub enable_supervision: bool,
    pub default_mailbox_capacity: usize,
    pub restart_strategy: RestartStrategy,
    pub shutdown_timeout: Duration,
    pub metrics_enabled: bool,
}

impl Default for ActorSystemConfig {
    fn default() -> Self {
        Self {
            enable_supervision: true,
            default_mailbox_capacity: 1000,
            restart_strategy: RestartStrategy::default(),
            shutdown_timeout: Duration::from_secs(30),
            metrics_enabled: true,
        }
    }
}

/// Restart strategy for failed actors
#[derive(Debug, Clone)]
pub enum RestartStrategy {
    /// Always restart immediately
    Always,
    
    /// Never restart
    Never,
    
    /// Restart with exponential backoff
    ExponentialBackoff {
        initial_delay: Duration,
        max_delay: Duration,
        multiplier: f64,
        max_restarts: Option<usize>,
    },
    
    /// Restart with fixed delay
    FixedDelay {
        delay: Duration,
        max_restarts: Option<usize>,
    },
}

impl Default for RestartStrategy {
    fn default() -> Self {
        RestartStrategy::ExponentialBackoff {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(60),
            multiplier: 2.0,
            max_restarts: Some(10),
        }
    }
}
```

2. **Implement Root Supervisor**
```rust
// src/actors/supervisor.rs

use super::*;
use crate::metrics::ACTOR_RESTARTS;

/// Root supervisor managing all actors in the system
pub struct RootSupervisor {
    config: ActorSystemConfig,
    registry: Arc<RwLock<ActorRegistry>>,
    supervised_actors: HashMap<String, SupervisedActor>,
    system: System,
}

struct SupervisedActor {
    name: String,
    addr: Box<dyn Any + Send>,
    restart_strategy: RestartStrategy,
    restart_count: usize,
    last_restart: Option<Instant>,
    health_check: Option<Box<dyn Fn() -> BoxFuture<'static, bool> + Send>>,
}

impl RootSupervisor {
    pub fn new(config: ActorSystemConfig) -> Self {
        let system = System::new();
        
        Self {
            config,
            registry: Arc::new(RwLock::new(ActorRegistry::new())),
            supervised_actors: HashMap::new(),
            system,
        }
    }
    
    /// Start the actor system with core actors
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting actor system");
        
        // Start system arbiter for background tasks
        let arbiter = Arbiter::new();
        
        // Start metrics collector if enabled
        if self.config.metrics_enabled {
            self.start_metrics_collector(&arbiter).await?;
        }
        
        // Start health monitor
        self.start_health_monitor(&arbiter).await?;
        
        info!("Actor system started successfully");
        Ok(())
    }
    
    /// Spawn a supervised actor
    pub fn spawn_supervised<A, F>(&mut self, 
        name: String,
        factory: F,
        strategy: Option<RestartStrategy>,
    ) -> Addr<A>
    where
        A: Actor<Context = Context<A>> + Supervised,
        F: Fn() -> A + Send + 'static,
    {
        let strategy = strategy.unwrap_or_else(|| self.config.restart_strategy.clone());
        let registry = self.registry.clone();
        let name_clone = name.clone();
        
        let addr = Supervisor::start_in_arbiter(&Arbiter::new().handle(), move |ctx| {
            let actor = factory();
            
            // Configure supervision
            ctx.set_mailbox_capacity(self.config.default_mailbox_capacity);
            
            // Register with system
            let registry = registry.clone();
            let name = name_clone.clone();
            ctx.run_later(Duration::from_millis(10), move |_, _| {
                let registry = registry.clone();
                let name = name.clone();
                tokio::spawn(async move {
                    let mut reg = registry.write().await;
                    reg.register(name, addr);
                });
            });
            
            actor
        });
        
        // Track supervised actor
        self.supervised_actors.insert(name.clone(), SupervisedActor {
            name: name.clone(),
            addr: Box::new(addr.clone()),
            restart_strategy: strategy,
            restart_count: 0,
            last_restart: None,
            health_check: None,
        });
        
        addr
    }
    
    /// Handle actor failure and potential restart
    async fn handle_actor_failure(&mut self, actor_name: &str, error: ActorError) {
        error!("Actor {} failed: {:?}", actor_name, error);
        
        if let Some(supervised) = self.supervised_actors.get_mut(actor_name) {
            supervised.restart_count += 1;
            ACTOR_RESTARTS.inc();
            
            match &supervised.restart_strategy {
                RestartStrategy::Never => {
                    warn!("Actor {} will not be restarted (strategy: Never)", actor_name);
                }
                RestartStrategy::Always => {
                    info!("Restarting actor {} immediately", actor_name);
                    self.restart_actor(actor_name).await;
                }
                RestartStrategy::ExponentialBackoff { initial_delay, max_delay, multiplier, max_restarts } => {
                    if let Some(max) = max_restarts {
                        if supervised.restart_count > *max {
                            error!("Actor {} exceeded max restarts ({})", actor_name, max);
                            return;
                        }
                    }
                    
                    let delay = calculate_backoff_delay(
                        supervised.restart_count,
                        *initial_delay,
                        *max_delay,
                        *multiplier,
                    );
                    
                    info!("Restarting actor {} after {:?}", actor_name, delay);
                    tokio::time::sleep(delay).await;
                    self.restart_actor(actor_name).await;
                }
                RestartStrategy::FixedDelay { delay, max_restarts } => {
                    if let Some(max) = max_restarts {
                        if supervised.restart_count > *max {
                            error!("Actor {} exceeded max restarts ({})", actor_name, max);
                            return;
                        }
                    }
                    
                    info!("Restarting actor {} after {:?}", actor_name, delay);
                    tokio::time::sleep(*delay).await;
                    self.restart_actor(actor_name).await;
                }
            }
            
            supervised.last_restart = Some(Instant::now());
        }
    }
    
    /// Gracefully shutdown the actor system
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Initiating actor system shutdown");
        
        let shutdown_deadline = Instant::now() + self.config.shutdown_timeout;
        
        // Send shutdown signal to all actors
        for (name, supervised) in &self.supervised_actors {
            debug!("Sending shutdown signal to actor {}", name);
            // Actor-specific shutdown logic would go here
        }
        
        // Wait for actors to finish with timeout
        while Instant::now() < shutdown_deadline {
            if self.all_actors_stopped().await {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        
        // Force stop any remaining actors
        if !self.all_actors_stopped().await {
            warn!("Force stopping remaining actors");
            self.system.stop();
        }
        
        info!("Actor system shutdown complete");
        Ok(())
    }
}

fn calculate_backoff_delay(
    attempt: usize,
    initial: Duration,
    max: Duration,
    multiplier: f64,
) -> Duration {
    let delay_ms = initial.as_millis() as f64 * multiplier.powi(attempt as i32 - 1);
    let delay_ms = delay_ms.min(max.as_millis() as f64);
    Duration::from_millis(delay_ms as u64)
}
```

3. **Implement Actor Registry**
```rust
// src/actors/registry.rs

use super::*;
use std::any::TypeId;

/// Registry for actor discovery and communication
pub struct ActorRegistry {
    actors: HashMap<String, ActorEntry>,
    type_index: HashMap<TypeId, Vec<String>>,
}

struct ActorEntry {
    name: String,
    addr: Box<dyn Any + Send>,
    actor_type: TypeId,
    created_at: Instant,
    message_count: AtomicUsize,
}

impl ActorRegistry {
    pub fn new() -> Self {
        Self {
            actors: HashMap::new(),
            type_index: HashMap::new(),
        }
    }
    
    /// Register an actor with the registry
    pub fn register<A: Actor>(&mut self, name: String, addr: Addr<A>) -> Result<()> {
        let type_id = TypeId::of::<A>();
        
        if self.actors.contains_key(&name) {
            return Err(Error::ActorAlreadyRegistered(name));
        }
        
        let entry = ActorEntry {
            name: name.clone(),
            addr: Box::new(addr),
            actor_type: type_id,
            created_at: Instant::now(),
            message_count: AtomicUsize::new(0),
        };
        
        self.actors.insert(name.clone(), entry);
        self.type_index.entry(type_id)
            .or_insert_with(Vec::new)
            .push(name);
        
        Ok(())
    }
    
    /// Get an actor by name
    pub fn get<A: Actor>(&self, name: &str) -> Option<Addr<A>> {
        self.actors.get(name)
            .and_then(|entry| entry.addr.downcast_ref::<Addr<A>>())
            .cloned()
    }
    
    /// Get all actors of a specific type
    pub fn get_by_type<A: Actor>(&self) -> Vec<Addr<A>> {
        let type_id = TypeId::of::<A>();
        
        self.type_index.get(&type_id)
            .map(|names| {
                names.iter()
                    .filter_map(|name| self.get::<A>(name))
                    .collect()
            })
            .unwrap_or_default()
    }
    
    /// Remove an actor from the registry
    pub fn unregister(&mut self, name: &str) -> Result<()> {
        if let Some(entry) = self.actors.remove(name) {
            if let Some(names) = self.type_index.get_mut(&entry.actor_type) {
                names.retain(|n| n != name);
            }
            Ok(())
        } else {
            Err(Error::ActorNotFound(name.to_string()))
        }
    }
}
```

4. **Create Legacy Adapter Pattern**
```rust
// src/actors/adapters.rs

use super::*;
use crate::chain::Chain;
use crate::engine::Engine;

/// Adapter to bridge legacy Arc<RwLock<T>> code with actor system
pub struct LegacyAdapter<T> {
    legacy: Arc<RwLock<T>>,
    actor: Option<Addr<dyn Actor>>,
}

impl<T> LegacyAdapter<T> {
    pub fn new(legacy: Arc<RwLock<T>>) -> Self {
        Self {
            legacy,
            actor: None,
        }
    }
    
    pub fn with_actor<A: Actor>(mut self, actor: Addr<A>) -> Self {
        self.actor = Some(Box::new(actor) as Box<dyn Actor>);
        self
    }
}

/// Chain adapter for gradual migration
pub struct ChainAdapter {
    legacy_chain: Arc<RwLock<Chain>>,
    chain_actor: Option<Addr<ChainActor>>,
    feature_flags: Arc<FeatureFlagManager>,
}

impl ChainAdapter {
    pub async fn import_block(&self, block: SignedConsensusBlock) -> Result<()> {
        if self.feature_flags.is_enabled("actor_system").await {
            // Use actor-based implementation
            if let Some(actor) = &self.chain_actor {
                actor.send(ImportBlock { block }).await?
            } else {
                return Err(Error::ActorNotInitialized);
            }
        } else {
            // Use legacy implementation
            self.legacy_chain.write().await.import_block(block).await
        }
    }
    
    pub async fn produce_block(&self) -> Result<SignedConsensusBlock> {
        if self.feature_flags.is_enabled("actor_system").await {
            if let Some(actor) = &self.chain_actor {
                actor.send(ProduceBlock).await?
            } else {
                return Err(Error::ActorNotInitialized);
            }
        } else {
            self.legacy_chain.write().await.produce_block().await
        }
    }
}
```

5. **Implement Health Monitoring**
```rust
// src/actors/health.rs

use super::*;

pub struct HealthMonitor {
    supervised_actors: Vec<String>,
    check_interval: Duration,
    unhealthy_threshold: usize,
}

impl Actor for HealthMonitor {
    type Context = Context<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.run_interval(self.check_interval, |act, ctx| {
            for actor_name in &act.supervised_actors {
                let name = actor_name.clone();
                ctx.spawn(
                    async move {
                        act.check_actor_health(name).await
                    }
                    .into_actor(act)
                    .map(|_, _, _| ())
                );
            }
        });
    }
}

impl HealthMonitor {
    async fn check_actor_health(&mut self, actor_name: String) {
        // Send health check message to actor
        // Track failures
        // Trigger restart if unhealthy
    }
}
```

6. **Create Integration Tests**
```rust
// tests/actor_system_test.rs

#[cfg(test)]
mod tests {
    use super::*;
    
    #[actix::test]
    async fn test_actor_supervision() {
        let config = ActorSystemConfig::default();
        let mut supervisor = RootSupervisor::new(config);
        
        // Start a test actor that will panic
        let addr = supervisor.spawn_supervised(
            "test_actor".to_string(),
            || PanickingActor::new(),
            Some(RestartStrategy::Always),
        );
        
        // Send message that causes panic
        addr.send(CausePanic).await.unwrap();
        
        // Wait for restart
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // Verify actor was restarted and is responsive
        let response = addr.send(Ping).await.unwrap();
        assert_eq!(response, "pong");
    }
    
    #[actix::test]
    async fn test_exponential_backoff() {
        let config = ActorSystemConfig::default();
        let mut supervisor = RootSupervisor::new(config);
        
        let strategy = RestartStrategy::ExponentialBackoff {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(1),
            multiplier: 2.0,
            max_restarts: Some(3),
        };
        
        let addr = supervisor.spawn_supervised(
            "backoff_actor".to_string(),
            || PanickingActor::new(),
            Some(strategy),
        );
        
        // Cause multiple panics and measure restart delays
        for i in 0..3 {
            let start = Instant::now();
            addr.send(CausePanic).await.ok();
            tokio::time::sleep(Duration::from_secs(2)).await;
            let elapsed = start.elapsed();
            
            // Verify exponential backoff
            let expected_delay = Duration::from_millis(100 * 2_u64.pow(i));
            assert!(elapsed >= expected_delay);
        }
    }
    
    #[actix::test]
    async fn test_graceful_shutdown() {
        let config = ActorSystemConfig {
            shutdown_timeout: Duration::from_secs(5),
            ..Default::default()
        };
        let mut supervisor = RootSupervisor::new(config);
        
        // Start multiple actors
        for i in 0..10 {
            supervisor.spawn_supervised(
                format!("actor_{}", i),
                || TestActor::new(),
                None,
            );
        }
        
        // Initiate shutdown
        let start = Instant::now();
        supervisor.shutdown().await.unwrap();
        let elapsed = start.elapsed();
        
        // Verify shutdown completed within timeout
        assert!(elapsed < Duration::from_secs(5));
    }
}
```

## Testing Plan

### Unit Tests
1. Test supervisor creation and configuration
2. Test restart strategies (always, never, backoff, fixed)
3. Test actor registration and discovery
4. Test mailbox overflow handling
5. Test health monitoring

### Integration Tests
1. Test full actor system with multiple actors
2. Test cascade failures and recovery
3. Test message routing between actors
4. Test legacy adapter pattern
5. Test gradual migration with feature flags

### Performance Tests
```rust
#[bench]
fn bench_actor_message_throughput(b: &mut Bencher) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let system = System::new();
    
    b.iter(|| {
        runtime.block_on(async {
            let actor = ThroughputTestActor::new();
            let addr = actor.start();
            
            // Send 10,000 messages
            let futures: Vec<_> = (0..10_000)
                .map(|i| addr.send(TestMessage { id: i }))
                .collect();
            
            futures::future::join_all(futures).await;
        })
    });
}
```

### Chaos Tests
1. Random actor failures
2. Message loss simulation
3. Mailbox overflow scenarios
4. Supervisor failure and recovery

## Dependencies

### Blockers
- ALYS-004: Feature flags needed for gradual migration

### Blocked By
- ALYS-001: Backup system for state recovery
- ALYS-002: Testing framework
- ALYS-003: Metrics infrastructure

### Related Issues
- ALYS-007: ChainActor implementation
- ALYS-008: EngineActor implementation
- ALYS-009: BridgeActor implementation

## Definition of Done

- [ ] Supervisor implementation complete
- [ ] All restart strategies working
- [ ] Actor registry operational
- [ ] Legacy adapters tested
- [ ] Health monitoring active
- [ ] Metrics integrated
- [ ] Performance benchmarks pass
- [ ] Documentation complete
- [ ] Code review by 2+ developers

## Notes

- Consider using Bastion or andere actor frameworks if Actix limitations found
- Implement circuit breakers for failing actors
- Add distributed tracing support
- Consider actor persistence for stateful actors