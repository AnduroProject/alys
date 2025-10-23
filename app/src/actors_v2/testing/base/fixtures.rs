use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Common test data fixtures and generators
#[derive(Debug, Clone)]
pub struct TestFixtures {
    pub test_data: HashMap<String, TestData>,
    pub seed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestData {
    String(String),
    Number(i64),
    Float(f64),
    Boolean(bool),
    Binary(Vec<u8>),
    Json(serde_json::Value),
}

impl TestFixtures {
    pub fn new() -> Self {
        Self {
            test_data: HashMap::new(),
            seed: 42, // Deterministic seed for reproducible tests
        }
    }

    pub fn with_seed(seed: u64) -> Self {
        Self {
            test_data: HashMap::new(),
            seed,
        }
    }

    pub fn add_string(&mut self, key: &str, value: String) {
        self.test_data
            .insert(key.to_string(), TestData::String(value));
    }

    pub fn add_number(&mut self, key: &str, value: i64) {
        self.test_data
            .insert(key.to_string(), TestData::Number(value));
    }

    pub fn add_binary(&mut self, key: &str, value: Vec<u8>) {
        self.test_data
            .insert(key.to_string(), TestData::Binary(value));
    }

    pub fn get_string(&self, key: &str) -> Option<&String> {
        match self.test_data.get(key) {
            Some(TestData::String(s)) => Some(s),
            _ => None,
        }
    }

    pub fn get_number(&self, key: &str) -> Option<i64> {
        match self.test_data.get(key) {
            Some(TestData::Number(n)) => Some(*n),
            _ => None,
        }
    }

    pub fn get_binary(&self, key: &str) -> Option<&Vec<u8>> {
        match self.test_data.get(key) {
            Some(TestData::Binary(b)) => Some(b),
            _ => None,
        }
    }

    /// Generate deterministic test data based on seed
    pub fn generate_deterministic_string(&self, length: usize, suffix: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.seed.hash(&mut hasher);
        suffix.hash(&mut hasher);
        let hash = hasher.finish();

        format!("test_{}_{:016x}", suffix, hash)
            .chars()
            .take(length)
            .collect()
    }

    pub fn generate_deterministic_bytes(&self, size: usize, suffix: &str) -> Vec<u8> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.seed.hash(&mut hasher);
        suffix.hash(&mut hasher);
        let mut hash = hasher.finish();

        let mut bytes = Vec::with_capacity(size);
        for _ in 0..size {
            bytes.push((hash & 0xff) as u8);
            hash = hash.wrapping_mul(1103515245).wrapping_add(12345);
        }
        bytes
    }
}

impl Default for TestFixtures {
    fn default() -> Self {
        Self::new()
    }
}

/// Common test patterns and builders
pub struct TestDataBuilder {
    fixtures: TestFixtures,
}

impl TestDataBuilder {
    pub fn new() -> Self {
        Self {
            fixtures: TestFixtures::new(),
        }
    }

    pub fn with_seed(seed: u64) -> Self {
        Self {
            fixtures: TestFixtures::with_seed(seed),
        }
    }

    pub fn add_test_strings(mut self, count: usize) -> Self {
        for i in 0..count {
            let key = format!("test_string_{}", i);
            let value = self.fixtures.generate_deterministic_string(20, &key);
            self.fixtures.add_string(&key, value);
        }
        self
    }

    pub fn add_test_numbers(mut self, count: usize) -> Self {
        for i in 0..count {
            let key = format!("test_number_{}", i);
            self.fixtures.add_number(&key, i as i64);
        }
        self
    }

    pub fn add_test_binary_data(mut self, count: usize, size: usize) -> Self {
        for i in 0..count {
            let key = format!("test_binary_{}", i);
            let value = self.fixtures.generate_deterministic_bytes(size, &key);
            self.fixtures.add_binary(&key, value);
        }
        self
    }

    pub fn build(self) -> TestFixtures {
        self.fixtures
    }
}

impl Default for TestDataBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Create common test fixtures for different scenarios
pub fn create_unit_test_fixtures() -> TestFixtures {
    TestDataBuilder::new()
        .add_test_strings(5)
        .add_test_numbers(5)
        .add_test_binary_data(3, 1024)
        .build()
}

pub fn create_integration_test_fixtures() -> TestFixtures {
    TestDataBuilder::new()
        .add_test_strings(50)
        .add_test_numbers(50)
        .add_test_binary_data(10, 10240)
        .build()
}

pub fn create_performance_test_fixtures() -> TestFixtures {
    TestDataBuilder::new()
        .add_test_strings(1000)
        .add_test_numbers(1000)
        .add_test_binary_data(100, 102400)
        .build()
}

/// Test environment setup utilities
pub fn setup_test_logging() {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .try_init()
        .ok(); // Ignore error if already initialized
}

/// Create temporary directories for testing
pub fn create_temp_dir() -> Result<tempfile::TempDir, std::io::Error> {
    tempfile::tempdir()
}

pub fn create_named_temp_dir(prefix: &str) -> Result<tempfile::TempDir, std::io::Error> {
    tempfile::Builder::new().prefix(prefix).tempdir()
}
