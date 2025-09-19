# GitHub Actions Workflows

## V2 Storage Actor Testing Pipeline

The `v2-storage-testing.yml` workflow provides comprehensive testing for the Storage Actor V2 implementation with the following layers:

### Testing Pyramid (Distribution)

- **Unit Tests (60%)**: Core functionality validation
- **Integration Tests (25%)**: Full actor lifecycle and interaction testing
- **Property Tests (10%)**: Property-based testing with random data generation
- **Chaos Tests (5%)**: Failure injection and resilience testing

### Workflow Stages

1. **Validate** - Code formatting, linting, and dependency checks
2. **Unit Tests** - Parallel execution of database, cache, message, and metrics tests
3. **Integration Tests** - Actor lifecycle, persistence, and concurrency testing
4. **Property Tests** - Automated property verification with 1000+ test cases
5. **Chaos Tests** - Failure injection scenarios (main branch only)
6. **Performance Tests** - Benchmarking with Criterion (main branch only)
7. **Coverage** - Test coverage reporting with Codecov integration
8. **Security Audit** - Dependency vulnerability scanning

### Triggers

- **Push**: `main`, `feature/v2-storage` branches with relevant file changes
- **Pull Request**: Against `main` branch with relevant file changes
- **Manual**: Chaos tests can be triggered with `chaos-test` label on PRs

### Environment Variables

- `RUST_BACKTRACE=1`: Full error backtraces
- `RUST_LOG`: Logging level control (debug/info/warn)
- `PROPTEST_CASES`: Number of property test cases (default: 1000)
- `CHAOS_TEST_DURATION`: Chaos test duration in seconds (default: 60)
- `CHAOS_FAILURE_RATE`: Failure injection rate (default: 0.15)

### Artifacts

- Benchmark results (uploaded for main branch)
- Coverage reports (sent to Codecov)
- Test summaries with detailed pass/fail status

### Matrix Strategy

Unit and integration tests use matrix execution for parallel processing:
- Unit tests: Split by test suite (database, cache, message, metrics)
- Integration tests: Split by functionality (actor, persistence, concurrency)

This ensures fast feedback and efficient CI resource utilization.