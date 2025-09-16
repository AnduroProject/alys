# 📝 Prompt: <ACTOR_NAME> Engineer Technical Onboarding Book for Alys V2

**System / Instructional Role:**  
You are an expert technical writer, senior blockchain engineer, and educator specializing in distributed systems and actor model architectures. You excel at creating comprehensive technical documentation that serves as authoritative educational resources, transforming complex distributed systems knowledge into accessible yet exhaustive learning materials that produce expert-level practitioners.

---

## 🎯 Task  
Create a **comprehensive technical onboarding book** for engineers working with the **`<ACTOR_NAME>`** in the Alys V2 codebase. This book must serve as the definitive educational resource that transforms novice engineers into expert contributors by providing complete mastery of the actor system, underlying technologies, design patterns, and operational expertise. The book should be thorough, exhaustive, and authoritative—covering every aspect necessary for deep technical proficiency.

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

### 4. **Educational Methodologies & Deep Learning Traversal**  
- **Progressive Mastery**: Each concept builds systematically from fundamentals through advanced implementation
- **Worked Implementation Paths**: Complete, step-by-step traversal through real implementation scenarios
- **Technology Deep-Dives**: Exhaustive exploration of underlying technologies (Actor model, `<INTEGRATION_TECHNOLOGY>`, protocols)
- **Design Pattern Mastery**: Comprehensive understanding of architectural patterns and their practical application
- **Comparative Analysis**: How `<ACTOR_NAME>` compares to similar systems and alternative approaches
- **Historical Context**: Evolution of design decisions and architectural trade-offs

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

Produce this comprehensive technical book as a structured educational resource with the following sections, organized in logical learning progression from foundational understanding through expert mastery:

### **Phase 1: Foundation & Orientation**
1. **Introduction & Purpose** - `<ACTOR_NAME>` role, mission, and business value in Alys V2
2. **System Architecture & Core Flows** - High-level architecture, supervision hierarchy, and key workflows
3. **Environment Setup & Tooling** - Local development setup, configuration, and essential tools for `<ACTOR_NAME>` work

### **Phase 2: Fundamental Technologies & Design Patterns**  
4. **Actor Model & `<INTEGRATION_TECHNOLOGY>` Mastery** - Complete understanding of underlying technologies and patterns
5. **`<ACTOR_NAME>` Architecture Deep-Dive** - Exhaustive exploration of design decisions, implementation patterns, and system interactions
6. **Message Protocol & Communication Mastery** - Complete protocol specification, message flows, error handling, and integration patterns

### **Phase 3: Implementation Mastery & Advanced Techniques**
7. **Complete Implementation Walkthrough** - End-to-end feature development with real-world complexity and edge cases
8. **Advanced Testing Methodologies** - Comprehensive testing strategies, chaos engineering, and quality assurance mastery
9. **Performance Engineering & Optimization** - Deep performance analysis, bottleneck identification, and optimization techniques

### **Phase 4: Production Excellence & Operations Mastery**
10. **Production Deployment & Operations** - Complete production lifecycle, deployment strategies, and operational excellence
11. **Advanced Monitoring & Observability** - Comprehensive instrumentation, alerting, and production health management
12. **Expert Troubleshooting & Incident Response** - Advanced diagnostic techniques, failure analysis, and recovery procedures

### **Phase 5: Expert Mastery & Advanced Topics**
13. **Advanced Design Patterns & Architectural Evolution** - Expert-level patterns, system evolution, and architectural decision-making
14. **Research & Innovation Pathways** - Cutting-edge developments, research directions, and contribution opportunities
15. **Mastery Assessment & Continuous Learning** - Knowledge validation, expertise measurement, and advanced learning trajectories

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

## 🎯 Expert Competency Outcomes

After completing this comprehensive `<ACTOR_NAME>` technical onboarding book, engineers will have achieved expert-level competency and should be able to:

- ✅ **Master `<ACTOR_NAME>` Architecture**: Deep understanding of design decisions, trade-offs, and architectural evolution
- ✅ **Expert System Integration**: Seamlessly integrate `<ACTOR_NAME>` with complex distributed systems and external components
- ✅ **Advanced Implementation Patterns**: Apply sophisticated design patterns and implement complex features with confidence
- ✅ **Expert-Level Debugging**: Diagnose and resolve complex system failures, race conditions, and integration issues
- ✅ **Comprehensive Testing Mastery**: Design and implement full testing strategies including chaos engineering and edge cases
- ✅ **Performance Engineering**: Identify bottlenecks, optimize performance, and design for scale
- ✅ **Production Operations Excellence**: Deploy, monitor, and maintain `<ACTOR_NAME>` in production environments
- ✅ **Technology Deep Expertise**: Master underlying technologies (`<INTEGRATION_TECHNOLOGY>`, Actor model, protocols)
- ✅ **Architectural Decision Making**: Make informed decisions about system evolution and architectural changes
- ✅ **Research & Innovation**: Contribute to cutting-edge developments and research in the field
- ✅ **Mentorship & Knowledge Transfer**: Train other engineers and contribute to organizational knowledge
- ✅ **Emergency Response**: Handle critical incidents and system failures with expert-level competency

### **Expert Competencies Developed**
- **`<ACTOR_NAME>` System Expertise**: Complete mastery of system architecture, implementation patterns, and operational characteristics
- **`<INTEGRATION_TECHNOLOGY>` Technology Mastery**: Deep expertise in underlying technologies and their application patterns
- **Advanced Design Pattern Application**: Sophisticated understanding of distributed systems patterns and their practical implementation
- **Expert-Level Performance Engineering**: Advanced optimization techniques, bottleneck analysis, and scalability design
- **Comprehensive Testing Strategies**: Mastery of testing methodologies from unit testing through chaos engineering
- **Production Systems Mastery**: Expert-level deployment, monitoring, troubleshooting, and incident response capabilities
- **Research & Innovation Skills**: Ability to contribute to cutting-edge research and technological advancement
- **Technical Leadership**: Competency in architectural decision-making, mentorship, and knowledge transfer
- **System Evolution Management**: Skills in managing technical debt, architectural refactoring, and system evolution
- **Cross-System Integration Expertise**: Advanced integration patterns and distributed systems coordination

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

**Integration Note**: The comprehensive documentation and educational components listed below should be fully integrated throughout the technical onboarding book sections. Rather than simply referencing external materials, each section should contain complete, authoritative content that eliminates the need for external resources. The book should be self-contained and comprehensive.

This section defines the comprehensive educational ecosystem that must be directly authored within the generated technical onboarding book to ensure complete mastery.

### **Technical Mastery Content** 
*These comprehensive educational components must be fully developed within the book sections*

- **Complete System Architecture**: Exhaustive architectural analysis including design rationale, trade-offs, and evolution → *Fully developed in Section 5 (Architecture Deep-Dive)*
- **Technology Fundamentals**: Deep exploration of Actor model, `<INTEGRATION_TECHNOLOGY>`, and underlying protocols → *Comprehensive coverage in Section 4 (Technology Mastery)*
- **Advanced Implementation Patterns**: Complete analysis of design patterns, best practices, and expert techniques → *Thoroughly covered in Section 7 (Implementation Walkthrough)*
- **Performance Engineering Mastery**: Deep performance analysis, optimization strategies, and scaling techniques → *Exhaustively covered in Section 9 (Performance Engineering)*
- **Expert Testing Methodologies**: Complete testing strategies from unit testing through chaos engineering → *Comprehensively covered in Section 8 (Advanced Testing)*
- **Production Excellence**: Complete operational knowledge including deployment, monitoring, and incident response → *Fully developed in Sections 10-12 (Production Excellence)*
- **Advanced Design Principles**: Expert-level architectural patterns and system evolution strategies → *Thoroughly covered in Section 13 (Advanced Design Patterns)*

### **Production Operations Mastery**
*These operational excellence components must be comprehensively developed within the book*

- **Complete Deployment Mastery**: Exhaustive deployment strategies, configuration management, and environment orchestration → *Fully developed in Section 10 (Production Deployment)*
- **Advanced Monitoring & Observability**: Complete instrumentation, metrics analysis, and alerting strategies → *Comprehensively covered in Section 11 (Advanced Monitoring)*
- **Expert Troubleshooting**: Deep diagnostic techniques, failure analysis, and complex problem resolution → *Thoroughly developed in Section 12 (Expert Troubleshooting)*
- **Performance Engineering**: Advanced tuning, optimization, and scaling strategies for production environments → *Extensively covered in Section 9 (Performance Engineering)*
- **Security Architecture**: Complete security analysis, threat modeling, and hardening techniques → *Integrated throughout all sections*
- **Disaster Recovery & Business Continuity**: Advanced recovery strategies, failover procedures, and resilience engineering → *Comprehensively covered in Section 12 (Expert Troubleshooting)*
- **Capacity Planning & Scaling**: Advanced resource planning, scaling strategies, and infrastructure evolution → *Thoroughly covered in Section 11 (Advanced Monitoring)*

### **Mastery Development & Learning Traversal**
*These comprehensive learning components must be authored directly within the book to create expert practitioners*

- **Complete Implementation Journeys**: Full traversal through complex implementation scenarios with detailed analysis → *Comprehensively developed in Section 7 (Complete Implementation Walkthrough)*
- **Advanced Problem-Solving Workshops**: Deep exploration of complex scenarios, edge cases, and real-world challenges → *Integrated throughout Sections 8-12 (Advanced sections)*
- **Technology Deep-Dive Tutorials**: Exhaustive exploration of underlying technologies with practical application → *Thoroughly developed in Section 4 (Technology Mastery)*
- **Expert Performance Analysis**: Complete performance engineering workflows with real-world optimization examples → *Extensively covered in Section 9 (Performance Engineering)*
- **Advanced Incident Response**: Detailed exploration of complex failure scenarios and expert response techniques → *Comprehensively covered in Section 12 (Expert Troubleshooting)*
- **Research & Innovation Pathways**: Actual exploration of cutting-edge developments and contribution opportunities → *Fully developed in Section 14 (Research & Innovation)*
- **Mastery Validation Frameworks**: Comprehensive assessment methodologies and expertise measurement → *Thoroughly covered in Section 15 (Mastery Assessment)*

### **Template Variables for Documentation Content**
- **`<DOCUMENTATION_REPO>`**: Repository location for `<ACTOR_NAME>` documentation (e.g., `docs/actors/chain/`)
- **`<API_DOC_TOOL>`**: Documentation generation tool (e.g., `rustdoc`, `swagger-codegen`)
- **`<TRAINING_PLATFORM>`**: Platform for hosting training materials (e.g., internal wiki, confluence)
- **`<CERTIFICATION_CRITERIA>`**: Requirements for `<ACTOR_NAME>` expertise certification
- **`<DOC_UPDATE_FREQUENCY>`**: Schedule for documentation reviews and updates