# 📝 Prompt: <ACTOR_NAME> Engineer Onboarding Guide Generation for Alys V2

**System / Instructional Role:**  
You are an expert technical writer, senior blockchain engineer, and educator specializing in distributed systems and actor model architectures. You excel at creating in-depth onboarding materials that accelerate new engineers' understanding of complex blockchain actor systems, consensus mechanisms, and fault-tolerant distributed architectures.

---

## 🎯 Task  
Create a **comprehensive onboarding guide** for engineers working with the **`<ACTOR_NAME>`** in the Alys V2 codebase. The guide must provide an **end-to-end understanding** of this specific actor: how it works, how its pieces fit together, and how to effectively debug and contribute to its implementation.

---

## 📚 Content Requirements  

### 1. **High-Level Orientation**  
- Purpose of `<ACTOR_NAME>` and its mission within the Alys V2 merged mining sidechain architecture
- Core user flow(s): `<CORE_USER_FLOW>` (e.g., Block Production Pipeline, Peg-in/Peg-out Processing, Mining Coordination)
- System architecture overview focused on `<ACTOR_NAME>` and its supervision hierarchy (include mermaid diagrams)
- Sequence of operations for `<KEY_WORKFLOWS>` (e.g., Block Import/Export, Consensus Voting, Federation Coordination)

### 2. **Knowledge Tree Structure**  
- **Roots**: Actor model fundamentals (Actix, message-passing, supervision), blockchain concepts specific to `<ACTOR_NAME>`
- **Trunk**: Main `<ACTOR_NAME>` modules (`<KEY_MODULES>` - e.g., config.rs, state.rs, messages.rs, handlers/)
- **Branches**: Subsystems/integrations relevant to `<ACTOR_NAME>` (supervision strategies, metrics collection, external integrations)
- **Leaves**: Implementation details (functions like `<FUNCTION_EXAMPLES>` - e.g., handle_block_import, validate_consensus, process_message)

### 3. **Codebase Walkthroughs**  
- Folder/file structure specific to `<ACTOR_NAME>` (e.g., `app/src/actors/chain/` for ChainActor)
- Integration points across `<KEY_MODULES>` and external systems (Bitcoin Core, Execution Layer, P2P Network)
- Example inputs/outputs for `<FUNCTION_EXAMPLES>` with real message types and data structures
- Procedural debugging examples for `<DEBUGGING_SCENARIO>` (e.g., actor restart cascades, message ordering failures, timing violations)

### 4. **Research-Backed Writing Practices**  
- Use chunking, progressive disclosure, worked examples, and dual-coding principles
- Provide checklists, cheatsheets, and hands-on exercises specific to `<ACTOR_NAME>`
- Include visual diagrams showing message flows, state transitions, and actor interactions
- Offer multiple learning paths for different experience levels

#### **Educational Aids & Visual Constructs**
Use these constructs when appropriate to enhance understanding:

- **Mermaid Diagrams**: Actor supervision hierarchies, message flow sequences, state transitions, system architecture overviews
- **Code Snippets**: Annotated examples with syntax highlighting, before/after comparisons, implementation patterns
- **Flowcharts**: Decision trees for debugging workflows, error handling paths, configuration choices
- **Sequence Diagrams**: Actor message interactions, integration workflows, timing-critical operations
- **Tables**: Message type comparisons, performance benchmarks, configuration options, error codes
- **Callout Boxes**: ⚠️ Warnings for critical timing constraints, 💡 Tips for optimization, 📝 Notes for important concepts
- **Interactive Checklists**: Setup verification steps, testing procedures, deployment readiness checks
- **ASCII Architecture Diagrams**: System topology, data flow visualization, component relationships
- **Timeline Visualizations**: Block production cycles, consensus rounds, recovery sequences
- **State Machine Diagrams**: Actor lifecycle states, consensus phases, error recovery flows

### 5. **Practical Engineering Aids**  
- Environment setup (`<DEV_ENV_SETUP>` - Local network with `<ACTOR_NAME>` configuration)
- Common commands/scripts specific to `<ACTOR_NAME>` testing and debugging
- Testing & CI/CD pipelines overview showing `<ACTOR_NAME>` test coverage
- Debugging workflows tailored to `<ACTOR_NAME>` failure modes
- Day 1 tasks for engineers working with `<ACTOR_NAME>`

---

## 🧪 Output Format  

Produce the guide as a structured document with the following sections:  

1. **Introduction & Purpose** - `<ACTOR_NAME>` role and mission in Alys V2
2. **System Architecture & Core Flows** - `<ACTOR_NAME>` architecture and key workflows  
3. **Knowledge Tree (progressive deep-dive)** - From fundamentals to advanced `<ACTOR_NAME>` concepts
4. **Codebase Walkthrough** - Detailed exploration of `<ACTOR_NAME>` implementation
5. **Procedural Debugging & Worked Examples** - Real debugging scenarios and solutions
6. **Environment Setup & Tooling** - Local development setup for `<ACTOR_NAME>` work
7. **Testing & CI/CD Integration** - `<ACTOR_NAME>` testing strategies and automation
8. **Pro Tips & Quick Reference** - Best practices and productivity shortcuts
9. **Glossary & Further Learning Paths** - Key terms and advanced resources

---

## 📋 `<ACTOR_NAME>` Specific Context for Alys V2

### **Actor Overview**
- **Primary Role**: `<ACTOR_PRIMARY_ROLE>` (e.g., Block production and consensus coordination for ChainActor)
- **Location**: `<ACTOR_LOCATION>` (e.g., `app/src/actors/chain/` for ChainActor)
- **Key Responsibilities**: `<KEY_RESPONSIBILITIES>` (e.g., Bitcoin integration, block validation, consensus timing)
- **External Dependencies**: `<EXTERNAL_DEPENDENCIES>` (e.g., Bitcoin Core RPC, Execution Layer, P2P Network)

### **Core Message Types for `<ACTOR_NAME>`**
- **Primary Messages**: `<PRIMARY_MESSAGES>` (e.g., `ProduceBlock`, `ValidateBlock`, `ProposeBlock`, `FinalizeBlock`)
- **Integration Messages**: `<INTEGRATION_MESSAGES>` (e.g., `BitcoinDeposit`, `ExecutionPayload`, `P2PMessage`)
- **Control Messages**: `<CONTROL_MESSAGES>` (e.g., `Restart`, `HealthCheck`, `ConfigUpdate`)
- **Error Messages**: `<ERROR_MESSAGES>` (e.g., `ValidationError`, `TimingViolation`, `IntegrationFailure`)

### **Performance Targets for `<ACTOR_NAME>`**
- **Message Throughput**: `<THROUGHPUT_TARGET>` (e.g., 1000+ concurrent messages per second)
- **Message Latency**: `<LATENCY_TARGET>` (e.g., Sub-100ms average processing time)
- **Recovery Time**: `<RECOVERY_TARGET>` (e.g., <5 second restart time)
- **Integration Response**: `<INTEGRATION_RESPONSE_TARGET>` (e.g., <1 second for external API calls)
- **Resource Usage**: `<RESOURCE_TARGET>` (e.g., <50MB memory footprint, <10% CPU under normal load)

### **Development Environment for `<ACTOR_NAME>`**
- **Local Setup Command**: `<LOCAL_SETUP_COMMAND>` (e.g., `./scripts/start_network.sh`)
- **Test Command**: `<TEST_COMMAND>` (e.g., `cargo test --lib chain_actor`)
- **Benchmark Command**: `<BENCHMARK_COMMAND>` (e.g., `cargo bench --bench chain_actor_benchmarks`)
- **Debug Configuration**: `<DEBUG_CONFIG>` (e.g., `RUST_LOG=chain_actor=debug`)
- **Key Config Files**: `<CONFIG_FILES>` (e.g., `etc/config/chain.json`, `app/src/actors/chain/config.rs`)

### **Integration Points for `<ACTOR_NAME>`**
- **Primary Integration**: `<PRIMARY_INTEGRATION>` (e.g., Bitcoin Core RPC for ChainActor)
- **Secondary Integrations**: `<SECONDARY_INTEGRATIONS>` (e.g., Execution Layer, P2P Network, Prometheus)
- **Data Flow In**: `<INPUT_DATA_FLOW>` (e.g., Bitcoin blocks, transaction pools, consensus messages)
- **Data Flow Out**: `<OUTPUT_DATA_FLOW>` (e.g., Signed blocks, validation results, health metrics)

### **Quality Gates for `<ACTOR_NAME>`**
- **Unit Tests**: `<UNIT_TEST_CRITERIA>` (e.g., 100% success rate for lifecycle and recovery testing)
- **Integration Tests**: `<INTEGRATION_TEST_CRITERIA>` (e.g., Full Bitcoin/Ethereum compatibility with <1% failure rate)
- **Performance Tests**: `<PERFORMANCE_TEST_CRITERIA>` (e.g., Maintain targets under 1000+ concurrent message load)
- **Chaos Tests**: `<CHAOS_TEST_CRITERIA>` (e.g., Automatic recovery within blockchain timing constraints)
- **End-to-End Tests**: `<E2E_TEST_CRITERIA>` (e.g., Complete block production cycle with external systems)

---

## 🎯 Expected Outcomes

After completing this `<ACTOR_NAME>` onboarding guide, engineers should be able to:

- ✅ **Understand `<ACTOR_NAME>` Architecture**: Complete comprehension of the actor's role, message flows, and integration points
- ✅ **Set up Local Development**: Configure development environment specifically for `<ACTOR_NAME>` work and testing
- ✅ **Implement `<ACTOR_NAME>` Features**: Add new functionality following Alys V2 patterns and `<ACTOR_NAME>` conventions
- ✅ **Debug `<ACTOR_NAME>` Issues**: Diagnose and resolve actor failures, message routing problems, and integration issues
- ✅ **Write `<ACTOR_NAME>` Tests**: Create comprehensive tests for lifecycle, message handling, and integration scenarios
- ✅ **Optimize `<ACTOR_NAME>` Performance**: Improve throughput, reduce latency, and handle high-load scenarios
- ✅ **Integrate with External Systems**: Successfully connect `<ACTOR_NAME>` with Bitcoin, Ethereum, and other components
- ✅ **Monitor `<ACTOR_NAME>` Health**: Set up monitoring, interpret metrics, and diagnose production issues
- ✅ **Contribute with Confidence**: Make robust contributions to `<ACTOR_NAME>` following best practices and quality gates

### **Key Skills Acquired**
- **`<ACTOR_NAME>` Implementation Patterns**: Understanding of actor-specific design patterns and conventions
- **Message Protocol Mastery**: Proficiency with `<ACTOR_NAME>`'s message types, flows, and error handling
- **Integration Expertise**: Knowledge of how `<ACTOR_NAME>` connects with external systems and other actors
- **Performance Optimization**: Skills to optimize `<ACTOR_NAME>` for production performance requirements
- **Testing Excellence**: Ability to create comprehensive test coverage for all `<ACTOR_NAME>` functionality