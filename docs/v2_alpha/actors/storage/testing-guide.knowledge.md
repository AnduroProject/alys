🧪 Storage Actor V2 Test Execution Guide

📋 Quick Reference Commands

# Navigate to the app directory
cd app

# Run all Storage Actor tests
cargo test --lib actors_v2::testing::storage

# Run specific test categories
cargo test --lib actors_v2::testing::storage::unit        # Unit tests
cargo test --lib actors_v2::testing::storage::integration # Integration tests
cargo test --lib actors_v2::testing::storage::property    # Property tests
cargo test --lib actors_v2::testing::storage::chaos       # Chaos tests

🎯 Detailed Test Categories

1. Unit Tests (60% of coverage)

# All unit tests
cargo test --lib actors_v2::testing::storage::unit

# Specific unit test suites
cargo test --lib actors_v2::testing::storage::unit::database_tests
cargo test --lib actors_v2::testing::storage::unit::cache_tests
cargo test --lib actors_v2::testing::storage::unit::message_tests
cargo test --lib actors_v2::testing::storage::unit::metrics_tests

# Run with output
cargo test --lib actors_v2::testing::storage::unit -- --nocapture

2. Integration Tests (25% of coverage)

# All integration tests
cargo test --lib actors_v2::testing::storage::integration

# Specific integration test suites
cargo test --lib actors_v2::testing::storage::integration::actor_tests
cargo test --lib actors_v2::testing::storage::integration::persistence_tests
cargo test --lib actors_v2::testing::storage::integration::concurrency_tests

# Run with single thread for concurrency safety
cargo test --lib actors_v2::testing::storage::integration -- --test-threads=1

3. Property Tests (10% of coverage)

# All property-based regression tests
cargo test --lib actors_v2::testing::storage::property

# Run with custom proptest settings
PROPTEST_CASES=1000 cargo test --lib actors_v2::testing::storage::property

# Specific property tests
cargo test --lib test_storage_retrieval_consistency
cargo test --lib test_state_idempotency
cargo test --lib test_block_height_ordering

4. Chaos Tests (5% of coverage)

# All chaos tests
cargo test --lib actors_v2::testing::storage::chaos

# Run with extended timeout
cargo test --lib actors_v2::testing::storage::chaos -- --test-threads=1

# Specific chaos scenarios
cargo test --lib test_network_partition_recovery
cargo test --lib test_disk_failure_resilience
cargo test --lib test_concurrent_operations_under_chaos

# Run with chaos configuration
CHAOS_TEST_DURATION=30 CHAOS_FAILURE_RATE=0.15 cargo test --lib actors_v2::testing::storage::chaos

🚀 Advanced Test Execution

Comprehensive Test Suite

# Run all Storage Actor tests with detailed output
cargo test --lib actors_v2::testing::storage -- --nocapture --test-threads=4

# Run with environment logging
RUST_LOG=debug cargo test --lib actors_v2::testing::storage

# Run with custom worker threads
TOKIO_WORKER_THREADS=8 cargo test --lib actors_v2::testing::storage

Performance and Load Testing

# Run tests with profiling
cargo test --lib actors_v2::testing::storage --release

# Run specific load tests
cargo test --lib test_database_large_data_handling
cargo test --lib test_concurrent_operations_under_chaos

# Memory usage testing
cargo test --lib test_memory_pressure_handling

CI/CD Simulation

# Simulate GitHub Actions workflow locally
cargo check --all-features
cargo fmt --all -- --check
cargo clippy --all-features -- -D warnings
cargo test --lib actors_v2::testing::storage -- --nocapture

🐛 Debugging and Troubleshooting

Debug Mode Testing

# Run with full backtraces
RUST_BACKTRACE=full cargo test --lib actors_v2::testing::storage

# Run single test with debug output
cargo test --lib test_database_block_storage_retrieval -- --nocapture --exact

# Run with tokio console (if enabled)
TOKIO_CONSOLE=1 cargo test --lib actors_v2::testing::storage

Test Data Management

# Clean test data
rm -rf /tmp/alys-v2-test-data

# Run with custom test data directory
ALYS_V2_TEST_DATA_DIR=/tmp/custom-test-data cargo test --lib actors_v2::testing::storage

📊 Test Coverage and Reporting

Coverage Analysis

# Install coverage tool
cargo install cargo-llvm-cov

# Generate coverage report
cargo llvm-cov --lib --workspace --html \
--ignore-filename-regex="(testing|test)" \
-- actors_v2::storage

# View coverage report
open target/llvm-cov/html/index.html

Test Metrics

# Run tests with timing
cargo test --lib actors_v2::testing::storage -- --report-time

# Run with custom test timeout
cargo test --lib actors_v2::testing::storage -- --timeout=300

🔧 Configuration Options

Environment Variables

export RUST_LOG=debug                    # Logging level
export TOKIO_WORKER_THREADS=4           # Async runtime threads
export PROPTEST_CASES=1000               # Property test iterations
export CHAOS_TEST_DURATION=60           # Chaos test duration (seconds)
export CHAOS_FAILURE_RATE=0.15          # Failure injection rate
export ALYS_V2_TEST_DATA_DIR=/tmp/test   # Test data directory

Test Filtering

# Run tests matching pattern
cargo test --lib storage_retrieval

# Exclude specific tests
cargo test --lib actors_v2::testing::storage -- --skip test_extended_chaos

# Run ignored tests
cargo test --lib actors_v2::testing::storage -- --ignored

📈 Continuous Integration

The GitHub Actions workflow at .github/workflows/v2-storage-testing.yml runs these tests automatically:

- Validation: Code formatting, linting, dependency checks
- Unit Tests: Parallel execution across test suites
- Integration Tests: Sequential execution for concurrency safety
- Property Tests: 1000 test cases with shrinking
- Chaos Tests: Main branch only, with system stress testing