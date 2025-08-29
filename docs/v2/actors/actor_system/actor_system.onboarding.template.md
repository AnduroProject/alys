# 📝 Actor System Engineer Onboarding Guide for Alys V2

**System / Instructional Role:**  
You are an expert technical writer, senior blockchain engineer, and educator specializing in distributed systems and actor model architectures. You excel at creating in-depth onboarding materials that accelerate new engineers' understanding of complex blockchain actor systems, consensus mechanisms, and fault-tolerant distributed architectures.

---

## 🎯 Task  
Create a **comprehensive onboarding guide** for engineers working with the **`actor_system`** crate in the Alys V2 codebase. The guide must provide an **end-to-end understanding** of this foundational crate: how it works, how its pieces fit together, and how to effectively debug and contribute to its implementation.

---

## 📚 Content Requirements  

### 1. **High-Level Orientation**  
- Purpose of `actor_system` crate and its mission within the Alys V2 merged mining sidechain architecture
- Core user flow(s): **Actor Lifecycle Management, Message Routing & Processing, Supervision & Recovery** 
- System architecture overview focused on `actor_system` and its supervision hierarchy (include mermaid diagrams)
- Sequence of operations for **Actor Registration, Message Handling, Error Recovery, Health Monitoring**

### 2. **Knowledge Tree Structure**  
- **Roots**: Actor model fundamentals (Actix, message-passing, supervision), blockchain-aware actor concepts
- **Trunk**: Main `actor_system` modules (actor.rs, supervisor.rs, mailbox.rs, message.rs, blockchain.rs, registry.rs)
- **Branches**: Subsystems/integrations (supervision strategies, metrics collection, blockchain event handling, lifecycle management)
- **Leaves**: Implementation details (functions like `handle_message`, `restart_actor`, `validate_blockchain_readiness`, `escalate_failure`)

### 3. **Codebase Walkthroughs**  
- Folder/file structure specific to `actor_system` (`crates/actor_system/src/`)
- Integration points across core modules and external systems (Actix runtime, blockchain components, monitoring systems)
- Example inputs/outputs for core functions with real message types and actor states
- Procedural debugging examples for **Actor Restart Cascades, Message Queue Overflow, Supervision Tree Failures**

### 4. **Research-Backed Writing Practices**  
- Use chunking, progressive disclosure, worked examples, and dual-coding principles
- Provide checklists, cheatsheets, and hands-on exercises specific to `actor_system`
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
- Environment setup: **Local testing environment with actor_system integration**
- Common commands/scripts specific to `actor_system` testing and debugging
- Testing & CI/CD pipelines overview showing `actor_system` test coverage
- Debugging workflows tailored to `actor_system` failure modes
- Day 1 tasks for engineers working with `actor_system`

---

## 🧪 Output Format  

Produce the guide as a structured document with the following sections:  

1. **Introduction & Purpose** - `actor_system` role and mission in Alys V2
2. **System Architecture & Core Flows** - `actor_system` architecture and key workflows  
3. **Knowledge Tree (progressive deep-dive)** - From fundamentals to advanced `actor_system` concepts
4. **Codebase Walkthrough** - Detailed exploration of `actor_system` implementation
5. **Procedural Debugging & Worked Examples** - Real debugging scenarios and solutions
6. **Environment Setup & Tooling** - Local development setup for `actor_system` work
7. **Testing & CI/CD Integration** - `actor_system` testing strategies and automation
8. **Pro Tips & Quick Reference** - Best practices and productivity shortcuts
9. **Glossary & Further Learning Paths** - Key terms and advanced resources

---

## 📋 `actor_system` Specific Context for Alys V2

### **Actor Overview**
- **Primary Role**: **Foundational actor framework providing blockchain-aware actor primitives, supervision, and message handling for all Alys V2 actors**
- **Location**: **`crates/actor_system/src/`**
- **Key Responsibilities**: **Actor lifecycle management, message routing, supervision trees, blockchain event coordination, fault tolerance, health monitoring**
- **External Dependencies**: **Actix runtime, Bitcoin Core integration points, Ethereum execution layer interfaces, metrics collection systems**

### **Core Message Types for `actor_system`**
- **Primary Messages**: **`HealthCheck`, `RestartActor`, `RegisterActor`, `UnregisterActor`, `MessageEnvelope`**
- **Integration Messages**: **`BlockchainEvent`, `CheckBlockchainReadiness`, `SubscribeToBlockchainEvents`** 
- **Control Messages**: **`SupervisorCommand`, `EscalateFailure`, `ActorStatusUpdate`, `ConfigUpdate`**
- **Error Messages**: **`ActorError`, `SupervisionError`, `MessageDeliveryFailed`, `HealthCheckFailed`**

### **Performance Targets for `actor_system`**
- **Message Throughput**: **10,000+ messages per second across all supervised actors**
- **Message Latency**: **Sub-10ms average message processing overhead**
- **Recovery Time**: **<500ms actor restart time for non-consensus actors, <100ms for consensus actors**
- **Integration Response**: **<50ms blockchain event propagation time**
- **Resource Usage**: **<5MB memory footprint per actor, <2% CPU overhead for supervision**

### **Development Environment for `actor_system`**
- **Local Setup Command**: **`cargo build -p actor_system && cargo test -p actor_system`**
- **Test Command**: **`cargo test -p actor_system --lib`**
- **Benchmark Command**: **`cargo bench -p actor_system`**
- **Debug Configuration**: **`RUST_LOG=actor_system=debug,actix=trace`**
- **Key Config Files**: **`crates/actor_system/src/config.rs`, test configurations in `src/testing.rs`**

### **Integration Points for `actor_system`**
- **Primary Integration**: **Actix runtime and actor framework foundation**
- **Secondary Integrations**: **Blockchain event systems, metrics collection (Prometheus), distributed tracing, health monitoring**
- **Data Flow In**: **Actor registration requests, health check responses, blockchain events, supervision commands**
- **Data Flow Out**: **Supervision decisions, health status reports, message routing confirmations, error escalations**

### **Quality Gates for `actor_system`**
- **Unit Tests**: **100% success rate for actor lifecycle, supervision, and message handling with comprehensive edge case coverage**
- **Integration Tests**: **Full compatibility with all Alys V2 actors (ChainActor, EngineActor, StorageActor, etc.) with <0.1% failure rate**
- **Performance Tests**: **Maintain throughput and latency targets under 1000+ concurrent actors with high message loads**
- **Chaos Tests**: **Automatic recovery from supervision tree failures, actor crashes, and resource exhaustion within timing constraints**
- **End-to-End Tests**: **Complete actor system functionality integrated with blockchain consensus and external system interfaces**

---

## 🎯 Expected Outcomes

After completing this `actor_system` onboarding guide, engineers should be able to:

- ✅ **Understand `actor_system` Architecture**: Complete comprehension of the foundational actor framework, supervision patterns, and blockchain integration
- ✅ **Set up Local Development**: Configure development environment specifically for `actor_system` work and comprehensive testing
- ✅ **Implement `actor_system` Features**: Add new actor primitives, supervision strategies, and blockchain-aware capabilities following Alys V2 patterns
- ✅ **Debug `actor_system` Issues**: Diagnose and resolve supervision failures, message routing problems, and actor lifecycle issues
- ✅ **Write `actor_system` Tests**: Create comprehensive tests for supervision trees, message handling, and blockchain integration scenarios
- ✅ **Optimize `actor_system` Performance**: Improve throughput, reduce latency, and handle high-load multi-actor scenarios
- ✅ **Integrate with Blockchain Systems**: Successfully connect `actor_system` with Bitcoin, Ethereum, and consensus components
- ✅ **Monitor `actor_system` Health**: Set up comprehensive monitoring, interpret supervision metrics, and diagnose production issues
- ✅ **Contribute with Confidence**: Make robust contributions to `actor_system` following best practices and maintaining system stability

### **Key Skills Acquired**
- **`actor_system` Implementation Patterns**: Deep understanding of actor framework design patterns, supervision strategies, and blockchain-aware actor concepts
- **Message Protocol Mastery**: Expert proficiency with `actor_system`'s message types, routing mechanisms, and error handling protocols
- **Integration Expertise**: Comprehensive knowledge of how `actor_system` provides foundation for all Alys V2 actors and external system integration
- **Performance Optimization**: Advanced skills to optimize `actor_system` for production performance under high-load multi-actor scenarios
- **Testing Excellence**: Ability to create exhaustive test coverage for all `actor_system` functionality including edge cases and failure scenarios

---

## 💡 Additional Context for Implementation

### **Core Modules Deep Dive**
- **`actor.rs`**: Base actor traits, lifecycle management, blockchain-aware extensions
- **`supervisor.rs`**: Supervision trees, restart strategies, escalation policies  
- **`mailbox.rs`**: Message queuing, priority handling, flow control
- **`message.rs`**: Message envelopes, correlation tracking, distributed tracing
- **`blockchain.rs`**: Blockchain-specific actor capabilities, timing constraints, federation support
- **`registry.rs`**: Actor registration, discovery, health monitoring
- **`error.rs`**: Comprehensive error handling, severity classification
- **`metrics.rs`**: Performance monitoring, health tracking, supervision analytics
- **`testing.rs`**: Test utilities, mock actors, chaos testing support

### **Blockchain Integration Specifics**
- **2-second block timing constraints** for consensus actors
- **Federation coordination** for multi-sig peg operations
- **AuxPoW finalization** event handling and propagation
- **Priority-based supervision** for consensus-critical vs background actors
- **Distributed tracing** correlation across actor boundaries for blockchain operations

### **Production Considerations**
- **Memory management** for long-running actor systems
- **Graceful shutdown** coordination across actor hierarchies  
- **Resource exhaustion** handling and recovery
- **Monitoring integration** with Prometheus and alerting systems
- **Performance tuning** for blockchain timing requirements