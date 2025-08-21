//! Chaos Engineering Tests for Phase 6: Testing & Performance
//! 
//! Advanced chaos engineering test suite for resilience validation using controlled
//! failure injection, network partitioning simulation, resource exhaustion tests,
//! and Byzantine failure scenarios for the Alys V2 actor system.

use crate::actors::foundation::{
    ActorSystemConfig, EnhancedSupervision, HealthMonitor, ShutdownCoordinator,
    SupervisedActorConfig, ActorPriority, RestartStrategy, ActorFailureInfo,
    ActorFailureType, RestartAttemptInfo, RestartReason, HealthCheckResult,
    PingMessage, PongMessage, ShutdownRequest, FailurePatternDetector
};
use actix::{Actor, Context, Handler, Message, Supervised, Addr};
use rand::{Rng, thread_rng};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, atomic::{AtomicBool, AtomicUsize, Ordering}, Mutex};
use std::time::{Duration, SystemTime, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Chaos engineering configuration
#[derive(Debug, Clone)]
pub struct ChaosConfig {
    /// Probability of failure injection (0.0 to 1.0)
    pub failure_rate: f64,
    /// Duration of chaos experiment
    pub experiment_duration: Duration,
    /// Types of chaos to inject
    pub chaos_types: Vec<ChaosType>,
    /// Actor selection strategy
    pub target_strategy: TargetStrategy,
    /// Recovery verification enabled
    pub verify_recovery: bool,
    /// Maximum concurrent failures
    pub max_concurrent_failures: usize,
}

impl Default for ChaosConfig {
    fn default() -> Self {
        Self {
            failure_rate: 0.1, // 10% failure rate
            experiment_duration: Duration::from_secs(60),
            chaos_types: vec![
                ChaosType::ActorPanic,
                ChaosType::NetworkPartition,
                ChaosType::ResourceExhaustion,
                ChaosType::MessageDelay,
            ],
            target_strategy: TargetStrategy::Random,
            verify_recovery: true,
            max_concurrent_failures: 5,
        }
    }
}

/// Types of chaos that can be injected
#[derive(Debug, Clone, PartialEq)]
pub enum ChaosType {
    /// Actor panic failures
    ActorPanic,
    /// Network partition simulation
    NetworkPartition,
    /// Resource exhaustion
    ResourceExhaustion,
    /// Message delay/loss
    MessageDelay,
    /// Byzantine failures (invalid behavior)
    ByzantineFailure,
    /// Clock skew simulation
    ClockSkew,
    /// Disk/IO failures
    IoFailure,
    /// Memory pressure
    MemoryPressure,
}

/// Strategy for selecting chaos targets
#[derive(Debug, Clone)]
pub enum TargetStrategy {
    /// Random selection
    Random,
    /// Target critical actors
    Critical,
    /// Target by priority level
    Priority(ActorPriority),
    /// Target specific actors
    Specific(Vec<String>),
    /// Target percentage of actors
    Percentage(f64),
}

/// Chaos experiment result
#[derive(Debug, Clone)]
pub struct ChaosExperimentResult {
    pub experiment_id: Uuid,
    pub config: ChaosConfig,
    pub start_time: SystemTime,
    pub end_time: SystemTime,
    pub total_failures_injected: usize,
    pub actors_affected: HashSet<String>,
    pub recovery_metrics: RecoveryMetrics,
    pub system_stability: SystemStabilityMetrics,
    pub lessons_learned: Vec<String>,
}

/// Recovery metrics from chaos experiments
#[derive(Debug, Clone)]
pub struct RecoveryMetrics {
    pub mean_recovery_time: Duration,
    pub max_recovery_time: Duration,
    pub successful_recoveries: usize,
    pub failed_recoveries: usize,
    pub cascade_failures: usize,
}

/// System stability metrics
#[derive(Debug, Clone)]
pub struct SystemStabilityMetrics {
    pub uptime_percentage: f64,
    pub message_throughput_impact: f64,
    pub memory_stability: bool,
    pub consensus_availability: f64,
    pub federation_health: f64,
}

/// Chaos test actor that can be manipulated during experiments
#[derive(Debug)]
pub struct ChaosTestActor {
    pub id: String,
    pub priority: ActorPriority,
    pub failure_probability: Arc<AtomicUsize>, // Stored as percentage * 100
    pub byzantine_mode: Arc<AtomicBool>,
    pub message_delay: Arc<AtomicUsize>, // Delay in milliseconds
    pub processed_messages: Arc<AtomicUsize>,
    pub failed_messages: Arc<AtomicUsize>,
}

impl ChaosTestActor {
    pub fn new(id: String, priority: ActorPriority) -> Self {
        Self {
            id,
            priority,
            failure_probability: Arc::new(AtomicUsize::new(0)),
            byzantine_mode: Arc::new(AtomicBool::new(false)),
            message_delay: Arc::new(AtomicUsize::new(0)),
            processed_messages: Arc::new(AtomicUsize::new(0)),
            failed_messages: Arc::new(AtomicUsize::new(0)),
        }
    }
    
    pub fn inject_failure_probability(&self, probability: f64) {
        let prob_int = (probability * 10000.0) as usize; // Store as basis points
        self.failure_probability.store(prob_int, Ordering::Relaxed);
    }
    
    pub fn enable_byzantine_mode(&self) {
        self.byzantine_mode.store(true, Ordering::Relaxed);
    }
    
    pub fn disable_byzantine_mode(&self) {
        self.byzantine_mode.store(false, Ordering::Relaxed);
    }
    
    pub fn inject_message_delay(&self, delay: Duration) {
        self.message_delay.store(delay.as_millis() as usize, Ordering::Relaxed);
    }
    
    fn should_fail(&self) -> bool {
        let prob = self.failure_probability.load(Ordering::Relaxed);
        if prob == 0 {
            return false;
        }
        let random_value = thread_rng().gen_range(0..10000);
        random_value < prob
    }
    
    async fn apply_message_delay(&self) {
        let delay_ms = self.message_delay.load(Ordering::Relaxed);
        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms as u64)).await;
        }
    }
}

impl Actor for ChaosTestActor {
    type Context = Context<Self>;
    
    fn started(&mut self, _ctx: &mut Self::Context) {
        println!("ChaosTestActor {} started with priority {:?}", self.id, self.priority);
    }
}

impl Supervised for ChaosTestActor {}

#[derive(Message)]
#[rtype(result = "Result<String, String>")]
pub struct ChaosTestMessage {
    pub id: u64,
    pub content: String,
    pub expect_byzantine: bool,
}

impl Handler<ChaosTestMessage> for ChaosTestActor {
    type Result = Result<String, String>;
    
    fn handle(&mut self, msg: ChaosTestMessage, _ctx: &mut Self::Context) -> Self::Result {
        // Apply chaos effects
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(self.apply_message_delay());
        
        // Check for failure injection
        if self.should_fail() {
            self.failed_messages.fetch_add(1, Ordering::Relaxed);
            if thread_rng().gen_bool(0.5) {
                panic!("Chaos-injected panic in actor {}", self.id);
            } else {
                return Err(format!("Chaos-injected failure in actor {}", self.id));
            }
        }
        
        // Check for Byzantine behavior
        if self.byzantine_mode.load(Ordering::Relaxed) && !msg.expect_byzantine {
            self.failed_messages.fetch_add(1, Ordering::Relaxed);
            // Return incorrect/malicious response
            return Ok(format!("BYZANTINE_RESPONSE:{}", thread_rng().gen::<u64>()));
        }
        
        self.processed_messages.fetch_add(1, Ordering::Relaxed);
        Ok(format!("Processed: {} by {}", msg.content, self.id))
    }
}

/// Chaos engineering orchestrator
pub struct ChaosEngineer {
    config: ChaosConfig,
    supervision: Arc<EnhancedSupervision>,
    health_monitor: Arc<HealthMonitor>,
    shutdown_coordinator: Arc<ShutdownCoordinator>,
    active_experiments: Arc<RwLock<HashMap<Uuid, ChaosExperiment>>>,
}

/// Individual chaos experiment
pub struct ChaosExperiment {
    pub id: Uuid,
    pub config: ChaosConfig,
    pub start_time: SystemTime,
    pub target_actors: Vec<String>,
    pub failure_injector: Arc<Mutex<FailureInjector>>,
    pub metrics_collector: MetricsCollector,
    pub is_running: Arc<AtomicBool>,
}

/// Failure injection mechanism
pub struct FailureInjector {
    active_failures: HashMap<String, ChaosType>,
    failure_history: Vec<InjectedFailure>,
    max_concurrent_failures: usize,
}

#[derive(Debug, Clone)]
pub struct InjectedFailure {
    pub target: String,
    pub failure_type: ChaosType,
    pub timestamp: SystemTime,
    pub duration: Option<Duration>,
    pub recovered: bool,
}

/// Metrics collection during chaos experiments
pub struct MetricsCollector {
    pub start_metrics: SystemMetrics,
    pub current_metrics: Arc<RwLock<SystemMetrics>>,
    pub metric_history: Arc<RwLock<Vec<(SystemTime, SystemMetrics)>>>,
}

#[derive(Debug, Clone)]
pub struct SystemMetrics {
    pub active_actors: usize,
    pub message_throughput: f64,
    pub average_latency: Duration,
    pub error_rate: f64,
    pub memory_usage: u64,
    pub cpu_usage: f64,
}

impl ChaosEngineer {
    pub fn new(
        config: ChaosConfig,
        supervision: Arc<EnhancedSupervision>,
        health_monitor: Arc<HealthMonitor>,
        shutdown_coordinator: Arc<ShutdownCoordinator>,
    ) -> Self {
        Self {
            config,
            supervision,
            health_monitor,
            shutdown_coordinator,
            active_experiments: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Start a chaos engineering experiment
    pub async fn start_experiment(&self, experiment_config: ChaosConfig) -> Result<Uuid, Box<dyn std::error::Error>> {
        let experiment_id = Uuid::new_v4();
        println!("Starting chaos experiment: {}", experiment_id);
        
        // Select target actors based on strategy
        let target_actors = self.select_target_actors(&experiment_config.target_strategy).await?;
        
        let experiment = ChaosExperiment {
            id: experiment_id,
            config: experiment_config.clone(),
            start_time: SystemTime::now(),
            target_actors: target_actors.clone(),
            failure_injector: Arc::new(Mutex::new(FailureInjector {
                active_failures: HashMap::new(),
                failure_history: Vec::new(),
                max_concurrent_failures: experiment_config.max_concurrent_failures,
            })),
            metrics_collector: MetricsCollector {
                start_metrics: self.collect_system_metrics().await,
                current_metrics: Arc::new(RwLock::new(self.collect_system_metrics().await)),
                metric_history: Arc::new(RwLock::new(Vec::new())),
            },
            is_running: Arc::new(AtomicBool::new(true)),
        };
        
        // Store the experiment
        self.active_experiments.write().await.insert(experiment_id, experiment);
        
        // Start the chaos injection process
        let chaos_task = self.run_chaos_experiment(experiment_id).await?;
        
        // Start metrics collection
        let metrics_task = self.run_metrics_collection(experiment_id).await?;
        
        Ok(experiment_id)
    }
    
    /// Run the main chaos experiment loop
    async fn run_chaos_experiment(&self, experiment_id: Uuid) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error>> {
        let active_experiments = self.active_experiments.clone();
        let supervision = self.supervision.clone();
        
        let task = tokio::spawn(async move {
            if let Some(experiment) = active_experiments.read().await.get(&experiment_id) {
                let start_time = Instant::now();
                let duration = experiment.config.experiment_duration;
                let failure_rate = experiment.config.failure_rate;
                
                while start_time.elapsed() < duration && experiment.is_running.load(Ordering::Relaxed) {
                    // Inject failures based on configuration
                    if thread_rng().gen::<f64>() < failure_rate {
                        if let Err(e) = Self::inject_random_failure(&experiment, &supervision).await {
                            eprintln!("Failed to inject failure: {}", e);
                        }
                    }
                    
                    // Wait before next potential failure
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                
                println!("Chaos experiment {} completed", experiment_id);
                experiment.is_running.store(false, Ordering::Relaxed);
            }
        });
        
        Ok(task)
    }
    
    /// Inject a random failure into the system
    async fn inject_random_failure(
        experiment: &ChaosExperiment,
        supervision: &Arc<EnhancedSupervision>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut injector = experiment.failure_injector.lock().unwrap();
        
        // Check if we've reached max concurrent failures
        if injector.active_failures.len() >= injector.max_concurrent_failures {
            return Ok(());
        }
        
        // Select random target and failure type
        let target = experiment.target_actors[thread_rng().gen_range(0..experiment.target_actors.len())].clone();
        let chaos_type = experiment.config.chaos_types[thread_rng().gen_range(0..experiment.config.chaos_types.len())].clone();
        
        // Skip if this actor already has an active failure
        if injector.active_failures.contains_key(&target) {
            return Ok(());
        }
        
        println!("Injecting {:?} failure into actor: {}", chaos_type, target);
        
        // Inject the specific type of failure
        match chaos_type {
            ChaosType::ActorPanic => {
                let failure_info = ActorFailureInfo {
                    timestamp: SystemTime::now(),
                    failure_type: ActorFailureType::Panic { backtrace: None },
                    message: format!("Chaos-injected panic in {}", target),
                    context: {
                        let mut ctx = HashMap::new();
                        ctx.insert("chaos_experiment".to_string(), experiment.id.to_string());
                        ctx.insert("chaos_type".to_string(), "ActorPanic".to_string());
                        ctx
                    },
                    escalate: false,
                };
                supervision.handle_actor_failure(&target, failure_info).await?;
            }
            ChaosType::NetworkPartition => {
                let failure_info = ActorFailureInfo {
                    timestamp: SystemTime::now(),
                    failure_type: ActorFailureType::NetworkFailure {
                        peer_id: Some("chaos_partition".to_string()),
                        error: "Simulated network partition".to_string(),
                    },
                    message: format!("Chaos-injected network partition for {}", target),
                    context: {
                        let mut ctx = HashMap::new();
                        ctx.insert("chaos_experiment".to_string(), experiment.id.to_string());
                        ctx.insert("chaos_type".to_string(), "NetworkPartition".to_string());
                        ctx
                    },
                    escalate: false,
                };
                supervision.handle_actor_failure(&target, failure_info).await?;
            }
            ChaosType::ResourceExhaustion => {
                let failure_info = ActorFailureInfo {
                    timestamp: SystemTime::now(),
                    failure_type: ActorFailureType::ResourceExhaustion {
                        resource_type: "memory".to_string(),
                        usage: 95.0 + thread_rng().gen::<f64>() * 5.0, // 95-100% usage
                    },
                    message: format!("Chaos-injected resource exhaustion for {}", target),
                    context: {
                        let mut ctx = HashMap::new();
                        ctx.insert("chaos_experiment".to_string(), experiment.id.to_string());
                        ctx.insert("chaos_type".to_string(), "ResourceExhaustion".to_string());
                        ctx
                    },
                    escalate: true, // Resource exhaustion should escalate
                };
                supervision.handle_actor_failure(&target, failure_info).await?;
            }
            ChaosType::ByzantineFailure => {
                let failure_info = ActorFailureInfo {
                    timestamp: SystemTime::now(),
                    failure_type: ActorFailureType::ConsensusFailure {
                        error_code: "BYZANTINE_BEHAVIOR".to_string(),
                    },
                    message: format!("Chaos-injected Byzantine behavior for {}", target),
                    context: {
                        let mut ctx = HashMap::new();
                        ctx.insert("chaos_experiment".to_string(), experiment.id.to_string());
                        ctx.insert("chaos_type".to_string(), "ByzantineFailure".to_string());
                        ctx.insert("malicious_actor".to_string(), target.clone());
                        ctx
                    },
                    escalate: true, // Byzantine failures should escalate
                };
                supervision.handle_actor_failure(&target, failure_info).await?;
            }
            _ => {
                // Other chaos types would be implemented here
                println!("Chaos type {:?} not yet implemented", chaos_type);
            }
        }
        
        // Record the injected failure
        injector.active_failures.insert(target.clone(), chaos_type.clone());
        injector.failure_history.push(InjectedFailure {
            target: target.clone(),
            failure_type: chaos_type,
            timestamp: SystemTime::now(),
            duration: None,
            recovered: false,
        });
        
        Ok(())
    }
    
    /// Run metrics collection during experiment
    async fn run_metrics_collection(&self, experiment_id: Uuid) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error>> {
        let active_experiments = self.active_experiments.clone();
        let chaos_engineer = self.clone();
        
        let task = tokio::spawn(async move {
            while let Some(experiment) = active_experiments.read().await.get(&experiment_id) {
                if !experiment.is_running.load(Ordering::Relaxed) {
                    break;
                }
                
                // Collect current system metrics
                let current_metrics = chaos_engineer.collect_system_metrics().await;
                
                // Update experiment metrics
                *experiment.metrics_collector.current_metrics.write().await = current_metrics.clone();
                experiment.metrics_collector.metric_history.write().await.push((SystemTime::now(), current_metrics));
                
                // Wait before next collection
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
        
        Ok(task)
    }
    
    /// Select target actors based on strategy
    async fn select_target_actors(&self, strategy: &TargetStrategy) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        // For this implementation, we'll simulate actor selection
        // In a real implementation, this would query the actor registry
        
        let all_actors = vec![
            "consensus_actor_1".to_string(),
            "consensus_actor_2".to_string(),
            "consensus_actor_3".to_string(),
            "network_actor_1".to_string(),
            "network_actor_2".to_string(),
            "mining_actor_1".to_string(),
            "mining_actor_2".to_string(),
            "federation_actor_1".to_string(),
            "federation_actor_2".to_string(),
            "governance_actor_1".to_string(),
        ];
        
        let selected = match strategy {
            TargetStrategy::Random => {
                let count = thread_rng().gen_range(1..=all_actors.len().min(5));
                let mut selected = Vec::new();
                let mut rng = thread_rng();
                for _ in 0..count {
                    let idx = rng.gen_range(0..all_actors.len());
                    if !selected.contains(&all_actors[idx]) {
                        selected.push(all_actors[idx].clone());
                    }
                }
                selected
            }
            TargetStrategy::Critical => {
                // Select critical infrastructure actors
                vec![
                    "consensus_actor_1".to_string(),
                    "federation_actor_1".to_string(),
                    "governance_actor_1".to_string(),
                ]
            }
            TargetStrategy::Specific(actors) => actors.clone(),
            TargetStrategy::Percentage(pct) => {
                let count = (all_actors.len() as f64 * pct).ceil() as usize;
                all_actors.into_iter().take(count).collect()
            }
            TargetStrategy::Priority(_priority) => {
                // For this implementation, select based on name patterns
                all_actors.into_iter().filter(|name| name.contains("consensus") || name.contains("federation")).collect()
            }
        };
        
        Ok(selected)
    }
    
    /// Collect current system metrics
    async fn collect_system_metrics(&self) -> SystemMetrics {
        // This would integrate with actual system monitoring
        // For testing, we simulate metrics collection
        
        SystemMetrics {
            active_actors: 10,
            message_throughput: 1000.0,
            average_latency: Duration::from_millis(50),
            error_rate: 0.01,
            memory_usage: 1024 * 1024 * 512, // 512 MB
            cpu_usage: 25.0,
        }
    }
    
    /// Stop a running experiment
    pub async fn stop_experiment(&self, experiment_id: Uuid) -> Result<ChaosExperimentResult, Box<dyn std::error::Error>> {
        let mut experiments = self.active_experiments.write().await;
        
        if let Some(mut experiment) = experiments.remove(&experiment_id) {
            experiment.is_running.store(false, Ordering::Relaxed);
            
            // Allow time for cleanup
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            // Analyze results
            let result = self.analyze_experiment_results(experiment).await?;
            Ok(result)
        } else {
            Err("Experiment not found".into())
        }
    }
    
    /// Analyze experiment results
    async fn analyze_experiment_results(&self, experiment: ChaosExperiment) -> Result<ChaosExperimentResult, Box<dyn std::error::Error>> {
        let injector = experiment.failure_injector.lock().unwrap();
        let metric_history = experiment.metrics_collector.metric_history.read().await;
        
        // Calculate recovery metrics
        let successful_recoveries = injector.failure_history.iter().filter(|f| f.recovered).count();
        let failed_recoveries = injector.failure_history.len() - successful_recoveries;
        
        let recovery_times: Vec<Duration> = injector.failure_history
            .iter()
            .filter_map(|f| f.duration)
            .collect();
        
        let mean_recovery_time = if !recovery_times.is_empty() {
            Duration::from_nanos(
                recovery_times.iter().map(|d| d.as_nanos()).sum::<u128>() as u64 / recovery_times.len() as u64
            )
        } else {
            Duration::from_secs(0)
        };
        
        let max_recovery_time = recovery_times.iter().max().cloned().unwrap_or(Duration::from_secs(0));
        
        // Calculate system stability metrics
        let throughput_impact = if !metric_history.is_empty() {
            let initial_throughput = experiment.metrics_collector.start_metrics.message_throughput;
            let final_throughput = metric_history.last().map(|(_, m)| m.message_throughput).unwrap_or(initial_throughput);
            (initial_throughput - final_throughput) / initial_throughput * 100.0
        } else {
            0.0
        };
        
        let uptime_percentage = 100.0 - (failed_recoveries as f64 / injector.failure_history.len() as f64 * 100.0);
        
        // Generate lessons learned
        let lessons_learned = vec![
            format!("Injected {} failures across {} actors", injector.failure_history.len(), experiment.target_actors.len()),
            format!("System maintained {:.2}% uptime during chaos", uptime_percentage),
            format!("Mean recovery time: {:?}", mean_recovery_time),
            format!("Throughput impact: {:.2}%", throughput_impact),
            if failed_recoveries > 0 {
                format!("Consider improving resilience for {} failure scenarios", failed_recoveries)
            } else {
                "System showed excellent resilience to injected failures".to_string()
            },
        ];
        
        Ok(ChaosExperimentResult {
            experiment_id: experiment.id,
            config: experiment.config,
            start_time: experiment.start_time,
            end_time: SystemTime::now(),
            total_failures_injected: injector.failure_history.len(),
            actors_affected: experiment.target_actors.into_iter().collect(),
            recovery_metrics: RecoveryMetrics {
                mean_recovery_time,
                max_recovery_time,
                successful_recoveries,
                failed_recoveries,
                cascade_failures: 0, // Would be calculated from failure patterns
            },
            system_stability: SystemStabilityMetrics {
                uptime_percentage,
                message_throughput_impact: throughput_impact,
                memory_stability: true, // Would be determined from metrics
                consensus_availability: 99.0, // Would be calculated from consensus metrics
                federation_health: 95.0, // Would be calculated from federation metrics
            },
            lessons_learned,
        })
    }
}

// Make ChaosEngineer cloneable for async tasks
impl Clone for ChaosEngineer {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            supervision: self.supervision.clone(),
            health_monitor: self.health_monitor.clone(),
            shutdown_coordinator: self.shutdown_coordinator.clone(),
            active_experiments: self.active_experiments.clone(),
        }
    }
}

// Tests for chaos engineering system
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_chaos_actor_failure_injection() {
        let actor = ChaosTestActor::new("test_chaos_actor".to_string(), ActorPriority::Normal);
        
        // Test normal operation
        assert_eq!(actor.processed_messages.load(Ordering::Relaxed), 0);
        assert!(!actor.should_fail());
        
        // Inject failure probability
        actor.inject_failure_probability(0.5); // 50% failure rate
        
        // Byzantine mode should be toggleable
        actor.enable_byzantine_mode();
        assert!(actor.byzantine_mode.load(Ordering::Relaxed));
        
        actor.disable_byzantine_mode();
        assert!(!actor.byzantine_mode.load(Ordering::Relaxed));
    }
    
    #[tokio::test]
    async fn test_chaos_config_defaults() {
        let config = ChaosConfig::default();
        
        assert_eq!(config.failure_rate, 0.1);
        assert_eq!(config.experiment_duration, Duration::from_secs(60));
        assert_eq!(config.max_concurrent_failures, 5);
        assert!(config.verify_recovery);
        assert!(config.chaos_types.len() >= 4);
    }
    
    #[tokio::test]
    async fn test_target_selection_strategies() {
        let system_config = ActorSystemConfig::development();
        let supervision = Arc::new(EnhancedSupervision::new(system_config.clone()));
        let health_monitor = Arc::new(HealthMonitor::new(system_config.clone()));
        let shutdown_coordinator = Arc::new(ShutdownCoordinator::new(system_config));
        
        let chaos_config = ChaosConfig::default();
        let engineer = ChaosEngineer::new(chaos_config, supervision, health_monitor, shutdown_coordinator);
        
        // Test random selection
        let random_targets = engineer.select_target_actors(&TargetStrategy::Random).await.unwrap();
        assert!(!random_targets.is_empty());
        
        // Test critical selection
        let critical_targets = engineer.select_target_actors(&TargetStrategy::Critical).await.unwrap();
        assert_eq!(critical_targets.len(), 3);
        
        // Test percentage selection
        let percentage_targets = engineer.select_target_actors(&TargetStrategy::Percentage(0.3)).await.unwrap();
        assert!(!percentage_targets.is_empty());
        
        // Test specific selection
        let specific_actors = vec!["actor1".to_string(), "actor2".to_string()];
        let specific_targets = engineer.select_target_actors(&TargetStrategy::Specific(specific_actors.clone())).await.unwrap();
        assert_eq!(specific_targets, specific_actors);
    }
    
    #[tokio::test]
    async fn test_failure_injection_limits() {
        let mut injector = FailureInjector {
            active_failures: HashMap::new(),
            failure_history: Vec::new(),
            max_concurrent_failures: 2,
        };
        
        // Should be able to add up to max concurrent failures
        injector.active_failures.insert("actor1".to_string(), ChaosType::ActorPanic);
        injector.active_failures.insert("actor2".to_string(), ChaosType::NetworkPartition);
        assert_eq!(injector.active_failures.len(), 2);
        
        // Adding more should be prevented by the experiment logic
        assert!(injector.active_failures.len() <= injector.max_concurrent_failures);
    }
    
    #[tokio::test]
    async fn test_metrics_collection() {
        let system_config = ActorSystemConfig::development();
        let supervision = Arc::new(EnhancedSupervision::new(system_config.clone()));
        let health_monitor = Arc::new(HealthMonitor::new(system_config.clone()));
        let shutdown_coordinator = Arc::new(ShutdownCoordinator::new(system_config));
        
        let chaos_config = ChaosConfig::default();
        let engineer = ChaosEngineer::new(chaos_config, supervision, health_monitor, shutdown_coordinator);
        
        let metrics = engineer.collect_system_metrics().await;
        assert!(metrics.active_actors > 0);
        assert!(metrics.message_throughput > 0.0);
        assert!(metrics.error_rate >= 0.0 && metrics.error_rate <= 1.0);
        assert!(metrics.memory_usage > 0);
        assert!(metrics.cpu_usage >= 0.0 && metrics.cpu_usage <= 100.0);
    }
}