# Revised Systematic Plan for Porting Storage Actor to V2

Based on your clarification, this is a much more straightforward porting task. The V2 system should maintain the sophisticated actor-based architecture while removing the custom `actor_system` dependency.

## Architecture Clarification

**V1 System:**
- Actix + custom `actor_system` crate + RocksDB
- Hybrid actor implementation

**V2 System:**
- Pure Actix (no `actor_system` crate) + RocksDB
- Standard Actix actor patterns

## Phase 1: Dependency Cleanup & Foundation

### 1.1 Remove Custom Actor System Dependencies
**From V1:**
```rust
use actor_system::{Actor as AlysActor, ActorMetrics, AlysActorMessage, ActorError};
```

**To V2:**
```rust
// Use standard Actix patterns only
use actix::prelude::*;
```

### 1.2 Dependencies to Keep/Add
- **Keep:** `actix`, `rocksdb`, `tracing`, `tokio`, `serde`
- **Keep:** All storage-related dependencies
- **Add to V2:** `rocksdb` (not currently in V2 Cargo.toml)
- **Remove:** `actor_system` references

### 1.3 Storage Backend (No Changes)
- **Keep:** RocksDB with existing column family structure
- **Keep:** All database operations and optimizations
- **Keep:** Sophisticated database configuration

## Phase 2: Pure Actix Actor Implementation

### 2.1 Actor Structure (Minimal Changes)
```rust
// V1 (hybrid approach)
#[derive(Debug)]
pub struct StorageActor {
    // ... existing fields
}

impl Actor for StorageActor {
    type Context = Context<Self>;
    // ... existing implementation
}

// V2 (pure Actix - keep same structure)
#[derive(Debug)]
pub struct StorageActor {
    // ... same fields, no changes needed
}

impl Actor for StorageActor {
    type Context = Context<Self>;
    // ... same implementation, remove actor_system calls
}
```

### 2.2 Remove Custom Actor System Integration
**Changes needed:**
- Remove `ActorMetrics` → Use direct metrics tracking
- Remove `AlysActorMessage` → Use standard Actix `Message` trait
- Remove `ActorError` → Use `StorageError` directly
- Remove any `actor_system` specific patterns

## Phase 3: Message System (Mostly Unchanged)

### 3.1 Keep All Message Types
**No changes needed for:**
- `StoreBlockMessage`
- `GetBlockMessage`
- `UpdateStateMessage`
- All 25+ message types from V1

### 3.2 Keep All Message Handlers
**Minimal changes to handlers:**
```rust
// V1 pattern - keep this
impl Handler<StoreBlockMessage> for StorageActor {
    type Result = ResponseActFuture<Self, Result<(), StorageError>>;

    fn handle(&mut self, msg: StoreBlockMessage, _: &mut Context<Self>) -> Self::Result {
        // Keep implementation, remove actor_system calls
    }
}
```

## Phase 4: Component Porting (Direct Migration)

### 4.1 Database Layer (`database.rs`)
- **Keep:** Entire `DatabaseManager` implementation
- **Keep:** All RocksDB operations and optimizations
- **Keep:** Column family structure and operations
- **No changes needed**

### 4.2 Caching Layer (`cache.rs`)
- **Keep:** Multi-level cache system
- **Keep:** LRU eviction and hit rate tracking
- **Keep:** Cache warming and maintenance
- **No changes needed**

### 4.3 Indexing System (`indexing.rs`)
- **Keep:** Advanced indexing capabilities
- **Keep:** Transaction/address/log indexing
- **Keep:** All indexing optimizations
- **No changes needed**

### 4.4 Metrics System (`metrics.rs`)
- **Keep:** Comprehensive metrics collection
- **Remove:** `ActorMetrics` dependency
- **Use:** Direct prometheus metrics (already in V2)

## Phase 5: Handler Cleanup

### 5.1 Remove Actor System Calls
**Pattern to follow:**
```rust
// V1 (remove actor_system calls)
self.metrics.record_actor_started(); // Remove if from actor_system

// V2 (use direct metrics)
self.metrics.record_startup(); // Direct metric call
```

### 5.2 AuxPow Integration (Keep As-Is)
- **Keep:** All AuxPow message handlers
- **Keep:** Difficulty history persistence
- **Keep:** Integration with AuxPow system

## Phase 6: File Structure

### 6.1 Directory Structure in `/app/src/actors_v2/storage/`
```
storage/
├── mod.rs          # Module exports
├── actor.rs        # Main StorageActor (from V1)
├── database.rs     # DatabaseManager (from V1)
├── cache.rs        # StorageCache (from V1)
├── indexing.rs     # StorageIndexing (from V1)
├── metrics.rs      # StorageActorMetrics (simplified)
├── messages.rs     # All message types (from V1)
└── handlers/       # Message handlers (from V1)
    ├── mod.rs
    ├── block_handlers.rs
    ├── state_handlers.rs
    ├── maintenance_handlers.rs
    └── query_handlers.rs
```

### 6.2 Integration with V2 Cargo.toml
**Add dependencies:**
```toml
# Add to V2's Cargo.toml
rocksdb = "0.21"  # Or appropriate version
actix = "0.13"    # Already may be present
```

## Phase 7: Testing (Direct Port)

### 7.1 Test Migration
- **Keep:** All existing unit tests from V1
- **Keep:** Integration tests, chaos tests, performance tests
- **Remove:** `actor_system` test dependencies
- **Update:** Test imports to remove `actor_system`

## Implementation Strategy

### Priority 1: Core Actor (Straightforward)
1. Copy V1 storage actor files to `/app/src/actors_v2/storage/`
2. Remove `actor_system` imports
3. Update Cargo.toml with RocksDB dependency
4. Fix compilation errors (should be minimal)

### Priority 2: Handler Cleanup (Simple)
1. Remove `actor_system` calls from handlers
2. Update metrics calls to direct prometheus calls
3. Test basic functionality

### Priority 3: Integration (Standard)
1. Integrate with V2's existing systems
2. Update any V2 systems that need to communicate with storage
3. Add RPC endpoints for storage queries

## Key Insight

This is primarily a **dependency cleanup task** rather than an architectural porting task. The sophisticated storage system, caching, indexing, and message handling can be preserved almost entirely as-is. The main work is removing the `actor_system` crate dependency and using pure Actix patterns.

**Estimated Effort:** Much lower than originally assessed - primarily find/replace operations and dependency cleanup rather than architectural redesign.