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
- Production deployment and operational procedures
- Monitoring setup and health check configurations
- Performance profiling and optimization workflows

---

## 🧪 Output Format  

Produce the guide as a structured document with the following sections, organized in logical learning progression:

### **Phase 1: Foundation & Orientation**
1. **Introduction & Purpose** - `<ACTOR_NAME>` role, mission, and business value in Alys V2
2. **System Architecture & Core Flows** - High-level architecture, supervision hierarchy, and key workflows
3. **Environment Setup & Tooling** - Local development setup, configuration, and essential tools for `<ACTOR_NAME>` work

### **Phase 2: Deep Technical Understanding**  
4. **Knowledge Tree (Progressive Deep-dive)** - From actor model fundamentals to advanced `<ACTOR_NAME>` concepts
5. **Codebase Walkthrough** - Detailed exploration of `<ACTOR_NAME>` implementation, modules, and integration points
6. **Message Protocol & Communication** - Complete message types, flows, and communication patterns

### **Phase 3: Practical Implementation**
7. **Hands-on Development Guide** - Step-by-step feature implementation following `<ACTOR_NAME>` patterns
8. **Testing & Quality Assurance** - Unit testing, integration testing, and quality gates for `<ACTOR_NAME>`
9. **Performance Optimization** - Profiling, benchmarking, and optimization techniques

### **Phase 4: Production & Operations**
10. **Monitoring & Observability** - Metrics, health checks, and production monitoring for `<ACTOR_NAME>`
11. **Debugging & Troubleshooting** - Diagnostic procedures, common issues, and resolution workflows  
12. **Documentation & Training Materials** - Comprehensive integration of developer docs, operational guides, and training resources (see Documentation and Training Framework section for required components)

### **Phase 5: Mastery & Reference**
13. **Pro Tips & Best Practices** - Expert techniques, optimization shortcuts, and productivity tips
14. **Quick Reference & Cheatsheets** - Commands, configurations, and troubleshooting checklists
15. **Glossary & Advanced Learning** - Key terms, concepts, and paths for continued learning

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
- **Security Tests**: `<SECURITY_TEST_CRITERIA>` (e.g., Vulnerability scanning and penetration testing)
- **Documentation Coverage**: `<DOC_COVERAGE_CRITERIA>` (e.g., 100% API documentation and architecture diagrams)

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
- ✅ **Access Comprehensive Documentation**: Utilize developer and operational documentation for effective `<ACTOR_NAME>` work
- ✅ **Complete Training Materials**: Execute hands-on exercises and workshops to master `<ACTOR_NAME>` implementation patterns
- ✅ **Deploy to Production**: Successfully deploy `<ACTOR_NAME>` to production environments with proper configuration
- ✅ **Implement Monitoring & Alerting**: Set up comprehensive observability for `<ACTOR_NAME>` health and performance
- ✅ **Handle Production Incidents**: Respond effectively to `<ACTOR_NAME>` failures and performance issues

### **Key Skills Acquired**
- **`<ACTOR_NAME>` Implementation Patterns**: Understanding of actor-specific design patterns and conventions
- **Message Protocol Mastery**: Proficiency with `<ACTOR_NAME>`'s message types, flows, and error handling
- **Integration Expertise**: Knowledge of how `<ACTOR_NAME>` connects with external systems and other actors
- **Performance Optimization**: Skills to optimize `<ACTOR_NAME>` for production performance requirements
- **Testing Excellence**: Ability to create comprehensive test coverage for all `<ACTOR_NAME>` functionality
- **Documentation Proficiency**: Competence in creating and maintaining technical documentation and training materials
- **Operational Excellence**: Skills in deployment, monitoring, and troubleshooting `<ACTOR_NAME>` in production environments
- **Production Readiness**: Ability to assess and ensure `<ACTOR_NAME>` production readiness across all quality gates
- **Incident Management**: Skills in incident detection, escalation, and resolution for `<ACTOR_NAME>` systems
- **Architecture Decision Making**: Competence in making informed architectural decisions for `<ACTOR_NAME>` evolution

---

## 🏗️ Template Usage Instructions

### **How to Use This Template**
1. **Replace Template Variables**: Search and replace all `<VARIABLE_NAME>` placeholders with actor-specific values
2. **Customize Content**: Adapt sections based on the specific actor's complexity and requirements  
3. **Validate Completeness**: Ensure all sections address the actor's unique characteristics and integration needs
4. **Review Learning Flow**: Verify the content follows logical progression from foundation to mastery

### **Key Template Variables Quick Reference**
- `<ACTOR_NAME>` - Name of the specific actor (e.g., ChainActor, NetworkActor, EngineActor)
- `<ACTOR_PRIMARY_ROLE>` - Main responsibility/purpose of the actor
- `<ACTOR_LOCATION>` - File system path where actor is implemented
- `<KEY_MODULES>` - Core modules/files for the actor
- `<INTEGRATION_TECHNOLOGY>` - Primary external integration (e.g., libp2p, Bitcoin Core)
- `<PRIMARY_MESSAGES>` - Main message types handled by the actor
- All performance, testing, and configuration variables as defined in context sections

---

## 📚 Documentation and Training Framework

**Integration Note**: The comprehensive documentation and training components listed below should be integrated throughout the onboarding guide sections as appropriate. Each deliverable section of the onboarding guide should incorporate relevant documentation types, operational guides, and training materials to ensure complete coverage.

This section defines the comprehensive documentation ecosystem that supports `<ACTOR_NAME>` development, operations, and knowledge transfer that must be included in the generated onboarding guide.

### **Developer Documentation** 
*These components should be integrated into relevant onboarding guide sections (Architecture, Codebase Walkthrough, Message Protocol, etc.)*

- **`<ACTOR_NAME>` Architecture Overview**: Comprehensive system design, component relationships, and integration patterns → *Include in Section 2 (System Architecture & Core Flows)*
- **Message Protocol Specification**: Complete `<ACTOR_NAME>` message types, flows, and communication patterns → *Include in Section 6 (Message Protocol & Communication)*
- **`<INTEGRATION_TECHNOLOGY>` Integration Patterns**: Best practices for integrating with external systems → *Include in Section 5 (Codebase Walkthrough)*
- **Performance Optimization Techniques**: Profiling methods, bottleneck identification, and optimization strategies → *Include in Section 9 (Performance Optimization)*
- **Testing and Debugging Guides**: Unit testing frameworks, integration testing patterns, and debugging methodologies → *Include in Sections 8, 11 (Testing, Debugging)*
- **API Reference Documentation**: Complete `<ACTOR_NAME>` API documentation with examples and usage patterns → *Include in Section 12 (Documentation & Training Materials)*
- **Code Style and Contribution Guidelines**: Standards for `<ACTOR_NAME>` development, code review, and contribution processes → *Include in Section 13 (Pro Tips & Best Practices)*

### **Operational Documentation**
*These components should be integrated into relevant onboarding guide sections (Environment Setup, Monitoring, Troubleshooting, etc.)*

- **Deployment and Configuration Guides**: Production deployment procedures, configuration management, and environment setup → *Include in Section 3 (Environment Setup & Tooling)*
- **Monitoring and Alerting Setup**: Metrics collection, dashboard configuration, and alerting rules for `<ACTOR_NAME>` health → *Include in Section 10 (Monitoring & Observability)*
- **Troubleshooting Common Issues**: Known issues, diagnostic procedures, and resolution steps for `<ACTOR_NAME>` failures → *Include in Section 11 (Debugging & Troubleshooting)*
- **Performance Tuning Recommendations**: Production optimization settings, resource allocation, and scaling strategies → *Include in Section 9 (Performance Optimization)*
- **Security Best Practices**: Security hardening, access control, and vulnerability mitigation → *Include in Sections 3, 12 (Environment Setup, Documentation)*
- **Disaster Recovery Procedures**: Backup strategies, failover processes, and recovery workflows → *Include in Section 11 (Debugging & Troubleshooting)*
- **Capacity Planning Guidelines**: Resource estimation, scaling indicators, and infrastructure requirements → *Include in Section 10 (Monitoring & Observability)*

### **Training Materials**
*These components should be integrated throughout the onboarding guide to provide hands-on learning experiences*

- **`<ACTOR_NAME>` System Walkthrough**: Interactive tutorials covering architecture, implementation, and operational aspects → *Integrate across Sections 2, 4, 5 (Architecture, Knowledge Tree, Codebase Walkthrough)*
- **Hands-on Implementation Exercises**: Practical coding exercises for implementing `<ACTOR_NAME>` features and integrations → *Include in Section 7 (Hands-on Development Guide)*
- **Integration Testing Workshops**: Guided workshops on testing `<ACTOR_NAME>` with external systems and other actors → *Include in Section 8 (Testing & Quality Assurance)*
- **Performance Analysis Techniques**: Training on profiling tools, performance measurement, and optimization workflows → *Include in Section 9 (Performance Optimization)*
- **Incident Response Procedures**: Emergency response protocols, escalation procedures, and recovery strategies → *Include in Section 11 (Debugging & Troubleshooting)*
- **Certification Pathways**: Structured learning tracks for different skill levels (Beginner, Intermediate, Advanced) → *Include in Section 15 (Glossary & Advanced Learning)*
- **Knowledge Validation Assessments**: Quizzes and practical exercises to validate understanding of `<ACTOR_NAME>` concepts → *Include throughout all sections as interactive elements*

### **Template Variables for Documentation Content**
- **`<DOCUMENTATION_REPO>`**: Repository location for `<ACTOR_NAME>` documentation (e.g., `docs/actors/chain/`)
- **`<API_DOC_TOOL>`**: Documentation generation tool (e.g., `rustdoc`, `swagger-codegen`)
- **`<TRAINING_PLATFORM>`**: Platform for hosting training materials (e.g., internal wiki, confluence)
- **`<CERTIFICATION_CRITERIA>`**: Requirements for `<ACTOR_NAME>` expertise certification
- **`<DOC_UPDATE_FREQUENCY>`**: Schedule for documentation reviews and updates