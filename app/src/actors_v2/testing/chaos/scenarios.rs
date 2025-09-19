use std::time::Duration;
use serde::{Serialize, Deserialize};

/// Simple chaos scenario enum for basic testing
#[derive(Debug, Clone, Copy)]
pub enum ChaosScenario {
    NetworkPartition,
    DiskFailure,
    MemoryPressure,
    ProcessCrash,
    SlowOperation,
}

impl ChaosScenario {
    /// Get description of the chaos scenario
    pub fn description(&self) -> &'static str {
        match self {
            ChaosScenario::NetworkPartition => "Network partition simulation",
            ChaosScenario::DiskFailure => "Disk I/O failure injection",
            ChaosScenario::MemoryPressure => "Memory pressure simulation",
            ChaosScenario::ProcessCrash => "Process crash and recovery",
            ChaosScenario::SlowOperation => "Operation slowdown injection",
        }
    }

    /// Get estimated duration for the scenario
    pub fn estimated_duration(&self) -> Duration {
        match self {
            ChaosScenario::NetworkPartition => Duration::from_secs(60),
            ChaosScenario::DiskFailure => Duration::from_secs(30),
            ChaosScenario::MemoryPressure => Duration::from_secs(30),
            ChaosScenario::ProcessCrash => Duration::from_secs(10),
            ChaosScenario::SlowOperation => Duration::from_secs(20),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioResult {
    pub scenario_name: String,
    pub duration: Duration,
    pub failures_injected: u64,
    pub failures_recovered: u64,
    pub system_resilient: bool,
    pub performance_impact: f64, // 0.0 = no impact, 1.0 = complete failure
    pub recovery_time: Option<Duration>,
    pub additional_metrics: std::collections::HashMap<String, f64>,
}