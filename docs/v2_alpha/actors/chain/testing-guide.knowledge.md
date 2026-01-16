🧪 ChainActor V2 Test Execution Guide

📋 Quick Reference Commands

# Navigate to the app directory
cd app

# Run all ChainActor tests
cargo test --lib actors_v2::testing::chain

# Run specific test categories
cargo test --lib actors_v2::testing::chain::unit        # Unit tests
cargo test --lib actors_v2::testing::chain::integration # Integration tests

🎯 Detailed Test Categories

1. Unit Tests (70% of coverage)

# All unit tests
cargo test --lib actors_v2::testing::chain::unit

# Specific unit test suites
cargo test --lib test_chain_config_validation
cargo test --lib test_chain_status_creation
cargo test --lib test_pegin_fixtures
cargo test --lib test_pegout_fixtures
cargo test --lib test_chain_state_creation
cargo test --lib test_chain_state_height_methods
cargo test --lib test_chain_state_sync_methods
cargo test --lib test_chain_state_auxpow_methods
cargo test --lib test_chain_state_queued_pow_methods
cargo test --lib test_chain_state_pegin_methods
cargo test --lib test_chain_state_edge_cases

# Run with output
cargo test --lib actors_v2::testing::chain::unit -- --nocapture

2. Integration Tests (30% of coverage)

# All integration tests
cargo test --lib actors_v2::testing::chain::integration

# Specific integration test suites
cargo test --lib test_chain_actor_basic_instantiation
cargo test --lib test_chain_actor_message_handling
cargo test --lib test_peg_operation_message_structure
cargo test --lib test_auxpow_message_structure
cargo test --lib test_block_message_variants
cargo test --lib test_query_message_variants
cargo test --lib test_chain_manager_message_variants
cargo test --lib test_chain_response_variants
cargo test --lib test_chain_status_response
cargo test --lib test_chain_manager_response_variants
cargo test --lib test_chain_state_transitions
cargo test --lib test_peg_operations_state_management
cargo test --lib test_chain_actor_instantiation_and_state
cargo test --lib test_actor_network_readiness_checks
cargo test --lib test_chain_actor_error_scenarios
cargo test --lib test_chain_error_conversions
cargo test --lib test_invalid_message_scenarios
cargo test --lib test_chain_state_error_conditions
cargo test --lib test_chain_config_error_scenarios
cargo test --lib test_test_harness_error_conditions
cargo test --lib test_concurrent_state_modifications
cargo test --lib test_chain_actor_address_management
cargo test --lib test_chain_actor_metrics_integration
cargo test --lib test_message_flow_patterns
cargo test --lib test_actor_lifecycle_integration
cargo test --lib test_integration_with_test_harness_variations
cargo test --lib test_cross_actor_data_consistency

# Run with single thread for concurrency safety
cargo test --lib actors_v2::testing::chain::integration -- --test-threads=1

🚀 Advanced Test Execution

Comprehensive Test Suite

# Run all ChainActor tests with detailed output
cargo test --lib actors_v2::testing::chain -- --nocapture --test-threads=4

# Run with environment logging
RUST_LOG=debug cargo test --lib actors_v2::testing::chain

# Run with custom worker threads
TOKIO_WORKER_THREADS=8 cargo test --lib actors_v2::testing::chain

Performance and Load Testing

# Run tests with profiling
cargo test --lib actors_v2::testing::chain --release

# Run specific load tests
cargo test --lib test_concurrent_state_modifications
cargo test --lib test_chain_actor_metrics_integration

# Memory usage testing
cargo test --lib test_chain_state_transitions

CI/CD Simulation

# Simulate GitHub Actions workflow locally
cargo check --all-features
cargo fmt --all -- --check
cargo clippy --all-features -- -D warnings
cargo test --lib actors_v2::testing::chain -- --nocapture

🐛 Debugging and Troubleshooting

Debug Mode Testing

# Run with full backtraces
RUST_BACKTRACE=full cargo test --lib actors_v2::testing::chain

# Run single test with debug output
cargo test --lib test_chain_state_creation -- --nocapture --exact

# Run with tokio console (if enabled)
TOKIO_CONSOLE=1 cargo test --lib actors_v2::testing::chain

Test Data Management

# Clean test data
rm -rf /tmp/alys-v2-chain-test-data

# Run with custom test data directory
ALYS_V2_TEST_DATA_DIR=/tmp/custom-chain-test-data cargo test --lib actors_v2::testing::chain

📊 Test Coverage and Reporting

Coverage Analysis

# Install coverage tool
cargo install cargo-llvm-cov

# Generate coverage report
cargo llvm-cov --lib --workspace --html \
--ignore-filename-regex="(testing|test)" \
-- actors_v2::chain

# View coverage report
open target/llvm-cov/html/index.html

Test Metrics

# Run tests with timing
cargo test --lib actors_v2::testing::chain -- --report-time

# Run with custom test timeout
cargo test --lib actors_v2::testing::chain -- --timeout=300

🔧 Configuration Options

Environment Variables

export RUST_LOG=debug                    # Logging level
export TOKIO_WORKER_THREADS=4           # Async runtime threads
export ALYS_V2_TEST_DATA_DIR=/tmp/test   # Test data directory

Test Filtering

# Run tests matching pattern
cargo test --lib chain_state

# Exclude specific tests
cargo test --lib actors_v2::testing::chain -- --skip test_concurrent_state_modifications

# Run ignored tests
cargo test --lib actors_v2::testing::chain -- --ignored

🎛️ ChainActor Specific Test Categories

Blockchain Core Tests

# Chain state management
cargo test --lib test_chain_state_transitions
cargo test --lib test_chain_state_height_methods
cargo test --lib test_chain_state_sync_methods

# Block operations
cargo test --lib test_block_message_variants
cargo test --lib test_query_message_variants

Consensus and Validation Tests

# AuxPoW functionality
cargo test --lib test_auxpow_message_structure
cargo test --lib test_chain_state_auxpow_methods
cargo test --lib test_chain_state_queued_pow_methods

# Consensus validation
cargo test --lib test_chain_config_validation
cargo test --lib test_chain_error_conversions

Peg Operations Tests

# Peg-in/Peg-out operations
cargo test --lib test_peg_operation_message_structure
cargo test --lib test_peg_operations_state_management
cargo test --lib test_pegin_fixtures
cargo test --lib test_pegout_fixtures

# Peg operation error handling
cargo test --lib test_invalid_message_scenarios

Actor Integration Tests

# Actor lifecycle and management
cargo test --lib test_actor_lifecycle_integration
cargo test --lib test_chain_actor_address_management
cargo test --lib test_actor_network_readiness_checks

# Cross-actor communication
cargo test --lib test_message_flow_patterns
cargo test --lib test_cross_actor_data_consistency

# Metrics and monitoring
cargo test --lib test_chain_actor_metrics_integration

Error Handling and Edge Cases

# Error scenarios
cargo test --lib test_chain_actor_error_scenarios
cargo test --lib test_chain_state_error_conditions
cargo test --lib test_chain_config_error_scenarios

# Edge cases and boundary conditions
cargo test --lib test_chain_state_edge_cases
cargo test --lib test_test_harness_error_conditions

Configuration and Setup Tests

# Test harness variations
cargo test --lib test_integration_with_test_harness_variations
cargo test --lib test_chain_actor_basic_instantiation

# Configuration validation
cargo test --lib test_chain_config_validation
cargo test --lib test_chain_config_error_scenarios

Concurrency and State Tests

# Concurrent operations
cargo test --lib test_concurrent_state_modifications

# State consistency
cargo test --lib test_chain_state_creation
cargo test --lib test_chain_actor_instantiation_and_state

📈 Test Architecture Overview

The ChainActor V2 testing framework includes:

Unit Tests (unit.rs):
- ChainConfig validation and creation
- ChainState basic operations and methods
- Test fixture validation
- Mock data consistency
- Address and Bitcoin data generation
- Basic state transitions

Integration Tests (integration.rs):
- Full ChainActor instantiation and lifecycle
- Message handling and response patterns
- Cross-actor communication simulation
- Error handling and recovery scenarios
- State persistence and consistency
- Metrics collection and reporting
- Concurrent operation testing

Test Harness (mod.rs):
- ChainTestHarness for complete setup
- Mock component creation (Engine, Aura, Bridge, etc.)
- Validator and non-validator configurations
- Component lifecycle management
- Resource cleanup and teardown

Fixtures (fixtures.rs):
- Pre-configured test data and settings
- Mock blockchain components
- Test addresses and identifiers
- Sample peg-in/peg-out operations
- AuxPoW test data

🔍 Test Execution Strategies

Development Workflow:
1. Run unit tests first for quick feedback
2. Run integration tests for component interaction
3. Use specific test filters during development
4. Enable logging for debugging complex scenarios

CI/CD Pipeline:
1. Unit tests run in parallel for speed
2. Integration tests run sequentially for consistency
3. Full test suite runs on main branch
4. Coverage reports generated for all tests

Performance Testing:
1. Use --release flag for performance tests
2. Monitor memory usage with custom test data
3. Test concurrent operations under load
4. Validate timeout configurations

📈 Continuous Integration

The GitHub Actions workflow runs these tests automatically:

- Validation: Code formatting, linting, dependency checks
- Unit Tests: Parallel execution across all test suites
- Integration Tests: Sequential execution for state consistency
- Coverage Analysis: Comprehensive test coverage reporting
- Performance Benchmarks: Basic performance regression detection

🚨 Common Issues and Solutions

Test Data Cleanup:
- Always clean up temporary directories after tests
- Use proper resource disposal in test harnesses
- Avoid test data contamination between runs

Concurrency Issues:
- Use --test-threads=1 for tests that modify global state
- Properly synchronize shared resources in tests
- Avoid race conditions in asynchronous test scenarios

Memory and Resource Management:
- Monitor memory usage during large test runs
- Clean up mock components properly
- Use appropriate timeouts for async operations

Mock Component Setup:
- Ensure all required mock components are properly initialized
- Use consistent test data across different test scenarios
- Validate mock component interactions match real behavior