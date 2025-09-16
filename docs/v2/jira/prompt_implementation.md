You are a senior Rust engineer implementing all subtasks from @docs/v2/jira/issue_12.md for the Alys V2 sidechain project. Use documentation in relevant `*.knowledge.md` files, Atlassian Jira task details, and Alys-specific architectural patterns to create a production-ready implementation.

## Implementation Requirements

### Primary Objective
Implement all subtasks with complete Rust code following Alys V2 architectural patterns, comprehensive testing using the Alys Testing Framework, and incremental git commits.

### Mandatory Deliverables
- Production-ready Rust implementation following Alys V2 patterns and best practices
- Comprehensive inline documentation with sidechain/governance domain context
- Unit tests integrated with Alys Testing Framework (>90% coverage)
- Integration tests using ActorTestHarness, SyncTestHarness, or relevant harnesses
- Property-based tests using PropTest generators where applicable
- Performance benchmarks using Criterion.rs integration
- Updated knowledge base documentation in `docs/v2/implementation_analysis/`
- Incremental git commits with Alys-specific commit message format
- Architecture diagrams using Mermaid showing sidechain and Anduro Governance stream interactions
- Chaos engineering tests for resilience validation (when applicable)

## Implementation Approach

### Phase 1: Analysis and Planning

#### 1. Deep Dive Analysis
- Read and analyze referenced `docs/v2/implementation_analysis/*.knowledge.md` files
- Parse Jira task from `docs/v2/jira/` acceptance criteria and subtask requirements
- Review relevant `app/src/` and `crates/` integration points
- Understand federation/consensus/governance stream context from `docs/knowledge/`
- Document architectural decisions considering Alys V2 migration constraints

#### 2. Implementation Strategy (Test-Driven Development)
- Break down phase into atomic, testable subtasks following TDD principles
- Write failing tests first using Alys Testing Framework components
- Define clear interfaces compatible with Actix actors and Tokio async patterns
- Plan error handling using `thiserror` and `anyhow` following Alys patterns
- Establish testing strategy using ActorTestHarness/SyncTestHarness/etc.
- Consider Anduro Governance stream integration and event processing requirements

### Phase 2: Incremental Implementation (TDD Workflow)

#### 1. For Each Subtask:
- Write failing tests first using appropriate Alys test harnesses
- Implement core functionality following Actix actor patterns where applicable
- Use Tokio async/await with proper error propagation
- Write comprehensive unit tests integrated with testing framework
- Add inline documentation explaining sidechain/governance event context
- Create integration tests using Docker test environment
- Run `cargo fmt`, `cargo clippy`, and `cargo check` before commits
- Commit changes following Alys commit message format (no AI references)

#### 2. Code Quality Standards (Alys-Specific):
- Follow Rust idioms with Alys V2 architectural patterns
- Use `thiserror` for custom error types with governance event domain context
- Implement Actix actor patterns for system components
- Use Tokio primitives (`spawn`, `timeout`, `select!`) appropriately
- Apply actor supervision patterns and graceful shutdown
- Ensure Anduro Governance stream compatibility and consensus safety
- Optimize for governance event processing and federation requirements

### Phase 3: Documentation and Knowledge Sharing

#### 1. Update Alys Documentation Files:
- Enhance `docs/v2/implementation_analysis/*.knowledge.md` with implementation details
- Add sidechain/governance-specific code examples and usage patterns
- Include troubleshooting for Anduro Governance stream and federation interactions
- Document performance characteristics for governance event processing operations
- Update root knowledge graphs (`docs/knowledge/`) if system-wide changes

#### 2. Create Comprehensive Guides:
- Step-by-step implementation walkthrough with governance stream context
- Architecture overview showing sidechain, federation, and Anduro Governance interactions
- Integration patterns with existing `app/src/` and `crates/` components
- Testing strategies using Alys Testing Framework harnesses
- Migration impact analysis for V1 to V2 transition

### Testing Framework (Alys Testing Framework Integration)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use alys_test_framework::{
        framework::{MigrationTestFramework, TestConfig},
        harness::{ActorTestHarness, SyncTestHarness},
        generators::*,
    };
    use tokio_test;
    use proptest::prelude::*;
    
    #[tokio::test]
    async fn test_governance_stream_integration_happy_path() {
        let config = TestConfig::development();
        let framework = MigrationTestFramework::new(config).unwrap();
        // Test implementation with Anduro Governance stream simulation
    }
    
    #[tokio::test]
    async fn test_federation_signature_validation() {
        let harness = ActorTestHarness::new().await;
        // Test federation signature handling with actor patterns
    }
    
    proptest! {
        #[test]
        fn test_property_based_validation(input in governance_event_strategy()) {
            // Property-based test using Alys generators
        }
    }
    
    #[tokio::test]
    async fn test_chaos_resilience() {
        // Chaos engineering test for governance stream failures
    }
}
```

## Performance and Optimization (Alys V2 Specific)

### Benchmarking (Criterion.rs Integration)

```rust
#[cfg(test)]
mod benchmarks {
    use criterion::{criterion_group, criterion_main, Criterion};
    use super::*;
    
    fn benchmark_governance_event_processing(c: &mut Criterion) {
        c.bench_function("governance_event_processing", |b| {
            b.iter(|| {
                // Governance event validation performance
                // Federation signature verification
                // Event stream processing
            })
        });
    }
    
    fn benchmark_federation_operations(c: &mut Criterion) {
        // BLS signature aggregation performance
        // Multi-signature threshold operations
        // Event-driven peg-in/peg-out processing throughput
    }
    
    criterion_group!(benches, benchmark_governance_event_processing, benchmark_federation_operations);
    criterion_main!(benches);
}
```

## Documentation Standards (Alys V2 Bitcoin Context)

### Mermaid Diagrams (Sidechain Focused)
Include relevant diagrams for:
- Alys sidechain architecture overview
- Sequence diagrams for event-driven peg-in/peg-out workflows
- Federation consensus state transitions
- Actor supervision hierarchy for governance event operations
- Anduro Governance stream communication patterns

Example (Federation Actor System):
```mermaid
graph TD
    A[Federation Supervisor] --> B[Governance Stream Monitor Actor]
    A --> C[Signature Aggregator Actor]
    A --> D[Peg Operation Actor]
    B --> E[Anduro Stream Client]
    C --> F[BLS Signature Pool]
    D --> G[Event Processor]
    D --> H[Transaction Builder]
```

### Knowledge Base Updates (docs/v2/implementation_analysis/)
Update `MODULE-NAME.knowledge.md` with:
- API documentation with governance stream/sidechain context
- Architecture decisions considering consensus safety
- Performance characteristics for governance event processing operations
- Integration patterns with existing `app/src/` and `crates/`
- Anduro Governance stream interaction patterns and best practices
- Federation signature workflow documentation
- Troubleshooting guides for governance stream issues
- Chaos engineering resilience patterns
- Testing framework integration examples
- Migration impact analysis and compatibility notes

## Git Commit Strategy (Alys-Specific)

### Pre-commit Quality Checks:
Run these commands before every commit:
```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo check --all-targets --all-features
cargo test --all
```

### Structure commits as:
```
feat(component): JIRA_ID-SUBTASK_NUMBER brief description

Detailed implementation notes with governance stream/sidechain context
Federation/consensus impact analysis
Performance impact on event processing operations
Testing coverage using Alys Testing Framework
Migration compatibility notes

Closes: JIRA_ID-SUBTASK_NUMBER
```

**Note:** Never reference AI assistance in commit messages per CLAUDE.md instructions.

## Quality Assurance Checklist (Alys V2 Specific)

Before completion, verify:

- ✅ All JIRA acceptance criteria met with governance stream/sidechain context
- ✅ Unit tests integrated with Alys Testing Framework (>90% coverage)
- ✅ Integration tests use appropriate harnesses (Actor/Sync/Network/etc.)
- ✅ Property-based tests written using Alys PropTest generators
- ✅ Performance benchmarks using Criterion.rs show acceptable metrics
- ✅ Chaos engineering tests validate resilience (where applicable)
- ✅ Error handling covers governance stream failures and edge cases
- ✅ Code follows Rust idioms with Actix/Tokio patterns
- ✅ Pre-commit checks pass (fmt, clippy, check, tests)
- ✅ Anduro Governance stream compatibility maintained
- ✅ Federation signature validation working correctly
- ✅ Governance event processing operations not disrupted
- ✅ Actor supervision and graceful shutdown implemented
- ✅ Documentation updated in `docs/v2/implementation_analysis/`
- ✅ Git commits follow Alys format (no AI references)
- ✅ Performance impact on governance event processing operations assessed

## Pro Tips for Alys V2 Implementation

### 1. Leverage Governance Domain Types:
- Use newtypes for governance event values (EventId, StreamOffset, etc.)
- Implement custom traits for federation operations
- Utilize type system for consensus safety guarantees
- Use const generics for cryptographic parameters

### 2. Actix/Tokio Best Practices:
- Use `Actor::start()` for supervised actor creation
- Implement proper message handling with error propagation
- Use `tokio::select!` for concurrent operation handling
- Implement graceful shutdown with federation state preservation
- Use `mpsc` channels for governance stream communication

### 3. Testing Strategies (Alys-Specific):
- Use `ActorTestHarness` for actor lifecycle testing
- Use `SyncTestHarness` for event stream synchronization tests
- Implement property tests for cryptographic operations
- Create chaos tests for governance stream failure scenarios
- Use Docker test environment for integration testing

### 4. Documentation Excellence (Governance Stream Context):
- Include Anduro Governance stream interaction examples
- Document federation signature requirements
- Explain consensus implications and safety properties
- Provide event-driven peg-in/peg-out workflow examples
- Document performance characteristics for event processing operations