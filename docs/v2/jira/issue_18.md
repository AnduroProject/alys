# ALYS-018: Documentation and Knowledge Transfer

## Issue Type
Task

## Priority
High

## Story Points
5

## Sprint
Migration Sprint 10

## Component
Documentation

## Labels
`migration`, `phase-10`, `documentation`, `training`, `knowledge-transfer`

## Description

Create comprehensive documentation for the migrated Alys v2 system and conduct knowledge transfer sessions. This includes technical documentation, operational guides, architectural diagrams, API documentation, and training materials for the team.

## Acceptance Criteria

- [ ] Technical documentation complete
- [ ] API documentation generated and published
- [ ] Architectural diagrams updated
- [ ] Operational runbooks finalized
- [ ] Training materials created
- [ ] Knowledge transfer sessions conducted
- [ ] Video tutorials recorded
- [ ] Documentation site deployed

## Technical Details

### Implementation Steps

1. **Documentation Site Setup**
```toml
# docs/book.toml
[book]
title = "Alys V2 Documentation"
authors = ["Alys Team"]
language = "en"
multilingual = false
src = "src"

[build]
build-dir = "book"

[preprocessor.index]

[preprocessor.links]

[preprocessor.mermaid]
command = "mdbook-mermaid"

[output.html]
theme = "theme"
default-theme = "rust"
preferred-dark-theme = "coal"
curly-quotes = true
mathjax-support = true
git-repository-url = "https://github.com/alys/alys-v2"
edit-url-template = "https://github.com/alys/alys-v2/edit/main/docs/{path}"

[output.html.fold]
enable = true
level = 0

[output.html.playground]
editable = true
copyable = true

[output.html.search]
enable = true
limit-results = 30
teaser-word-count = 30
use-boolean-and = true
boost-title = 2
boost-hierarchy = 1
boost-paragraph = 1
expand = true
heading-split-level = 3
```

2. **Architecture Documentation**
```markdown
# docs/src/architecture/overview.md

# Alys V2 Architecture Overview

## System Architecture

```mermaid
graph TB
    subgraph "External Systems"
        BTC[Bitcoin Network]
        ETH[Ethereum Network]
        GOV[Anduro Governance]
    end
    
    subgraph "Alys Core"
        subgraph "Actor System"
            SA[Supervisor Actor]
            CA[Chain Actor]
            BA[Bridge Actor]
            EA[Engine Actor]
            SY[Sync Actor]
            ST[Stream Actor]
        end
        
        subgraph "Consensus Layer"
            AURA[Aura PoA]
            POW[AuxPoW]
            LH[Lighthouse V5]
        end
        
        subgraph "Data Layer"
            DB[(PostgreSQL)]
            CACHE[(Redis)]
            IPFS[(IPFS)]
        end
    end
    
    BTC --> CA
    ETH --> EA
    GOV --> ST
    
    SA --> CA
    SA --> BA
    SA --> EA
    SA --> SY
    SA --> ST
    
    CA --> AURA
    CA --> POW
    EA --> LH
    
    BA --> DB
    SY --> DB
    EA --> CACHE
```

## Component Descriptions

### Actor System
The actor system is the heart of Alys V2, providing:
- **Fault isolation**: Each actor runs independently
- **Scalability**: Actors can be distributed across nodes
- **Resilience**: Supervisor ensures failed actors restart
- **Message passing**: Async communication between components

### Key Actors

#### ChainActor
- Manages blockchain state
- Coordinates with Bitcoin for merged mining
- Handles block production and validation
- **Location**: `src/actors/chain.rs`

#### BridgeActor
- Manages two-way peg operations
- Processes peg-ins and peg-outs
- Coordinates with governance for signatures
- **Location**: `src/actors/bridge.rs`

#### EngineActor
- Interfaces with execution layer (Geth/Reth)
- Manages EVM state transitions
- Handles transaction execution
- **Location**: `src/actors/engine.rs`

#### SyncActor
- Manages node synchronization
- Implements parallel block validation
- Handles chain reorganizations
- **Location**: `src/actors/sync.rs`

#### StreamActor
- Maintains governance connection
- Routes signature requests
- Handles federation updates
- **Location**: `src/actors/stream.rs`

## Data Flow

### Block Production Flow
1. ChainActor receives transactions from P2P network
2. Transactions validated and added to mempool
3. Aura PoA creates block proposal
4. EngineActor executes transactions via EVM
5. Block broadcast to network
6. Bitcoin miner includes block hash in coinbase
7. PoW confirmation finalizes block

### Peg-in Flow
1. User sends BTC to federation address
2. Bitcoin transaction detected by ChainActor
3. After 6 confirmations, BridgeActor initiates mint
4. EngineActor credits user's EVM address
5. Event emitted and logged

### Peg-out Flow
1. User burns tokens via bridge contract
2. EngineActor detects burn event
3. BridgeActor creates Bitcoin transaction
4. StreamActor requests signatures from governance
5. Signed transaction broadcast to Bitcoin network
```

3. **API Documentation Generator**
```rust
// docs/generate_api_docs.rs

use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

#[derive(OpenApi)]
#[openapi(
    paths(
        health_check,
        get_block,
        send_transaction,
        get_balance,
        estimate_gas,
    ),
    components(
        schemas(Block, Transaction, Balance, GasEstimate, Error)
    ),
    tags(
        (name = "Core", description = "Core blockchain operations"),
        (name = "Bridge", description = "Two-way peg operations"),
        (name = "Admin", description = "Administrative endpoints")
    ),
    info(
        title = "Alys V2 API",
        version = "2.0.0",
        description = "Alys sidechain JSON-RPC and REST API",
        contact(
            name = "Alys Team",
            email = "dev@alys.io",
            url = "https://alys.io"
        ),
        license(
            name = "MIT",
            url = "https://opensource.org/licenses/MIT"
        )
    )
)]
struct ApiDoc;

/// Health check endpoint
#[utoipa::path(
    get,
    path = "/health",
    tag = "Core",
    responses(
        (status = 200, description = "Service is healthy", body = HealthStatus),
        (status = 503, description = "Service is unhealthy", body = Error)
    )
)]
async fn health_check() -> Result<Json<HealthStatus>, Error> {
    // Implementation
}

/// Get block by height or hash
#[utoipa::path(
    get,
    path = "/block/{identifier}",
    tag = "Core",
    params(
        ("identifier" = String, Path, description = "Block height or hash")
    ),
    responses(
        (status = 200, description = "Block found", body = Block),
        (status = 404, description = "Block not found", body = Error)
    )
)]
async fn get_block(identifier: Path<String>) -> Result<Json<Block>, Error> {
    // Implementation
}

// Generate OpenAPI spec
fn generate_openapi_spec() {
    let openapi = ApiDoc::openapi();
    let spec = serde_json::to_string_pretty(&openapi).unwrap();
    std::fs::write("docs/api/openapi.json", spec).unwrap();
}

// Serve Swagger UI
async fn serve_swagger_ui() -> SwaggerUi {
    SwaggerUi::new("/swagger-ui/{_:.*}")
        .url("/api-doc/openapi.json", ApiDoc::openapi())
}
```

4. **Operational Guides**
```markdown
# docs/src/operations/deployment.md

# Deployment Guide

## Prerequisites

### System Requirements
- Ubuntu 22.04 LTS or later
- 8 CPU cores minimum
- 32GB RAM minimum
- 500GB SSD storage
- 100Mbps network connection

### Software Dependencies
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Install system dependencies
sudo apt-get update
sudo apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    postgresql-14 \
    redis-server \
    nginx

# Install Docker
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker $USER
```

## Deployment Steps

### 1. Clone Repository
```bash
git clone https://github.com/alys/alys-v2.git
cd alys-v2
git checkout v2.0.0
```

### 2. Build Application
```bash
# Production build
cargo build --release

# Run tests
cargo test --release

# Generate documentation
cargo doc --no-deps
```

### 3. Configure Services
```bash
# Copy configuration templates
cp etc/config/config.template.toml /etc/alys/config.toml
cp etc/systemd/alys.service /etc/systemd/system/

# Edit configuration
vim /etc/alys/config.toml
```

### 4. Database Setup
```sql
-- Create database and user
CREATE DATABASE alys;
CREATE USER alys WITH ENCRYPTED PASSWORD 'secure_password';
GRANT ALL PRIVILEGES ON DATABASE alys TO alys;

-- Run migrations
psql -U alys -d alys -f migrations/001_initial.sql
psql -U alys -d alys -f migrations/002_indexes.sql
```

### 5. Start Services
```bash
# Enable and start services
sudo systemctl daemon-reload
sudo systemctl enable alys
sudo systemctl start alys

# Check status
sudo systemctl status alys
journalctl -u alys -f
```

## Configuration Reference

### Main Configuration
```toml
# /etc/alys/config.toml

[node]
# Node identity
name = "alys-node-1"
chain_id = 263634

# Network settings
[network]
listen_addr = "0.0.0.0:30303"
external_addr = "1.2.3.4:30303"
bootnodes = [
    "/ip4/10.0.0.1/tcp/30303/p2p/QmNode1",
    "/ip4/10.0.0.2/tcp/30303/p2p/QmNode2"
]

# RPC settings
[rpc]
http_addr = "0.0.0.0:8545"
ws_addr = "0.0.0.0:8546"
max_connections = 1000

# Database settings
[database]
url = "postgresql://alys:password@localhost/alys"
max_connections = 50
min_connections = 10

# Consensus settings
[consensus]
engine = "aura"
authorities = [
    "0x1234...",
    "0x5678...",
    "0x9abc..."
]

# Bridge settings
[bridge]
bitcoin_rpc = "http://localhost:8332"
bitcoin_user = "rpcuser"
bitcoin_pass = "rpcpass"

# Governance settings
[governance]
endpoint = "governance.anduro.io:50051"
auth_token = "..."
```

## Monitoring Setup

### Prometheus Configuration
```yaml
# /etc/prometheus/prometheus.yml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: 'alys'
    static_configs:
      - targets: ['localhost:9090']
```

### Grafana Dashboard Import
1. Access Grafana at http://localhost:3000
2. Import dashboard from `monitoring/grafana/dashboard.json`
3. Configure data source to Prometheus

## Troubleshooting

### Common Issues

#### Node Won't Start
```bash
# Check logs
journalctl -u alys --since "1 hour ago"

# Verify configuration
alys config validate

# Check port availability
netstat -tulpn | grep -E "30303|8545"
```

#### Sync Issues
```bash
# Check peer connections
alys peers list

# Force resync
alys admin resync --from-height 0

# Clear corrupted database
systemctl stop alys
rm -rf /var/lib/alys/db/*
systemctl start alys
```

#### High Memory Usage
```bash
# Check memory stats
alys admin memory-stats

# Adjust cache settings
vim /etc/alys/config.toml
# Reduce cache_size_mb

# Restart with lower memory limit
systemctl edit alys
# Add: MemoryMax=16G
```
```

5. **Training Materials**
```markdown
# docs/src/training/overview.md

# Alys V2 Training Program

## Module 1: Architecture Overview (2 hours)

### Topics Covered
- Actor model fundamentals
- System architecture
- Component interactions
- Data flow patterns

### Hands-on Lab
1. Deploy local development environment
2. Explore actor system with monitoring tools
3. Trace a transaction through the system

### Resources
- [Architecture Slides](slides/architecture.pdf)
- [Actor Model Tutorial](https://doc.akka.io/docs/akka/current/typed/guide/actors-intro.html)
- [Video: System Overview](https://videos.alys.io/architecture)

## Module 2: Development Workflow (3 hours)

### Topics Covered
- Development environment setup
- Code structure and conventions
- Testing strategies
- Debugging techniques

### Hands-on Lab
1. Set up development environment
2. Write a simple actor
3. Add unit and integration tests
4. Debug with tracing

### Code Examples
```rust
// Example: Creating a new actor
use actix::prelude::*;

pub struct MyActor {
    counter: u64,
}

impl Actor for MyActor {
    type Context = Context<Self>;
}

#[derive(Message)]
#[rtype(result = "u64")]
pub struct GetCount;

impl Handler<GetCount> for MyActor {
    type Result = u64;
    
    fn handle(&mut self, _: GetCount, _: &mut Context<Self>) -> Self::Result {
        self.counter
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[actix::test]
    async fn test_counter() {
        let actor = MyActor { counter: 0 }.start();
        let count = actor.send(GetCount).await.unwrap();
        assert_eq!(count, 0);
    }
}
```

## Module 3: Operations (4 hours)

### Topics Covered
- Deployment procedures
- Monitoring and alerting
- Incident response
- Performance tuning

### Hands-on Lab
1. Deploy to staging environment
2. Set up monitoring dashboards
3. Simulate and resolve incidents
4. Optimize performance

### Runbook Examples
- [Emergency Response](../operations/emergency.md)
- [Performance Tuning](../operations/performance.md)
- [Backup and Recovery](../operations/backup.md)

## Module 4: Security (2 hours)

### Topics Covered
- Security architecture
- Key management
- Governance integration
- Audit procedures

### Security Checklist
- [ ] TLS enabled for all connections
- [ ] Authentication configured
- [ ] Secrets encrypted at rest
- [ ] Audit logging enabled
- [ ] Security scanning automated
- [ ] Incident response plan ready

## Module 5: Advanced Topics (3 hours)

### Topics Covered
- Lighthouse integration
- Governance HSM communication
- P2WSH implementation
- Performance optimization

### Deep Dive Sessions
1. Actor supervision strategies
2. Parallel sync implementation
3. Signature validation system
4. Database optimization

## Assessment

### Knowledge Check
1. Explain the actor model benefits
2. Describe the block production flow
3. List monitoring best practices
4. Demonstrate debugging techniques

### Practical Exercise
Build and deploy a simple feature:
1. Create new actor for the feature
2. Write comprehensive tests
3. Deploy to staging
4. Monitor performance
5. Document the feature

## Resources

### Documentation
- [Technical Docs](https://docs.alys.io)
- [API Reference](https://api.alys.io)
- [GitHub Repository](https://github.com/alys/alys-v2)

### Support Channels
- Slack: #alys-dev
- Email: dev@alys.io
- Office Hours: Tuesdays 2-4pm

### External Resources
- [Actix Documentation](https://actix.rs)
- [Lighthouse Documentation](https://lighthouse.sigmaprime.io)
- [Bitcoin Developer Guide](https://developer.bitcoin.org)
```

6. **Video Tutorial Scripts**
```markdown
# docs/video-scripts/01-getting-started.md

# Video 1: Getting Started with Alys V2

## Introduction (0:00-0:30)
"Welcome to Alys V2! In this video, we'll set up a local development environment and run your first node."

## Prerequisites (0:30-1:30)
[Screen: Terminal]
"First, let's install the prerequisites..."
```bash
# Show Rust installation
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install dependencies
sudo apt-get update && sudo apt-get install -y build-essential pkg-config libssl-dev
```

## Clone and Build (1:30-3:00)
[Screen: VS Code]
"Now let's clone the repository and build the project..."
```bash
git clone https://github.com/alys/alys-v2.git
cd alys-v2
cargo build --release
```

## Configuration (3:00-5:00)
[Screen: Config file]
"Let's configure our node..."
- Show config file structure
- Explain key settings
- Set up local testnet

## Running the Node (5:00-7:00)
[Screen: Terminal with logs]
"Start the node and watch it sync..."
```bash
./target/release/alys --config config.toml
```

## Verification (7:00-8:00)
[Screen: Browser with RPC calls]
"Let's verify our node is running..."
```bash
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'
```

## Conclusion (8:00-8:30)
"Congratulations! You now have Alys V2 running locally. In the next video, we'll explore the actor system."
```

## Testing Plan

### Documentation Validation
1. Technical accuracy review
2. Code example testing
3. Link verification
4. Spelling and grammar check

### Training Effectiveness
1. Pre-training assessment
2. Post-training assessment
3. Practical exercise evaluation
4. Feedback collection

## Dependencies

### Blockers
None

### Blocked By
- ALYS-016: Production deployment
- ALYS-017: Performance optimization

### Related Issues
- ALYS-019: Public documentation site
- ALYS-020: Community resources

## Definition of Done

- [ ] Technical documentation complete
- [ ] API documentation generated
- [ ] Architecture diagrams created
- [ ] Operational guides written
- [ ] Training materials prepared
- [ ] Video tutorials recorded
- [ ] Documentation site deployed
- [ ] Team training completed
- [ ] Feedback incorporated

## Notes

- Use mdBook for documentation site
- Record videos in 1080p minimum
- Include plenty of code examples
- Keep documentation up-to-date with code

## Time Tracking

- Estimated: 5 days
- Actual: _To be filled_