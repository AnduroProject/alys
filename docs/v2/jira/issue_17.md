# ALYS-017: Performance Optimization and Tuning

## Issue Type
Task

## Priority
High

## Story Points
8

## Sprint
Migration Sprint 9

## Component
Performance

## Labels
`migration`, `phase-9`, `performance`, `optimization`, `tuning`

## Description

Optimize the migrated Alys v2 system for production performance. This includes profiling, bottleneck identification, memory optimization, database tuning, and implementing performance improvements across all components.

## Acceptance Criteria

- [ ] Performance profiling complete
- [ ] Bottlenecks identified and resolved
- [ ] Memory usage reduced by 30%
- [ ] Transaction throughput increased by 50%
- [ ] P99 latency reduced below 100ms
- [ ] Database queries optimized
- [ ] Caching strategy implemented
- [ ] Resource utilization optimized

## Technical Details

### Implementation Steps

1. **Performance Profiling Infrastructure**
```rust
// src/profiling/mod.rs

use std::sync::Arc;
use tracing_subscriber::prelude::*;
use pprof::ProfilerGuard;

pub struct PerformanceProfiler {
    cpu_profiler: Option<ProfilerGuard<'static>>,
    memory_tracker: MemoryTracker,
    trace_collector: TraceCollector,
    metrics: Arc<ProfilingMetrics>,
}

impl PerformanceProfiler {
    pub fn new() -> Self {
        // Initialize tracing
        let tracer = opentelemetry_jaeger::new_pipeline()
            .with_service_name("alys-v2")
            .install_batch(opentelemetry::runtime::Tokio)
            .expect("Failed to initialize tracer");
        
        let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
        
        tracing_subscriber::registry()
            .with(telemetry)
            .with(tracing_subscriber::fmt::layer())
            .init();
        
        Self {
            cpu_profiler: None,
            memory_tracker: MemoryTracker::new(),
            trace_collector: TraceCollector::new(),
            metrics: Arc::new(ProfilingMetrics::new()),
        }
    }
    
    pub fn start_cpu_profiling(&mut self) -> Result<(), ProfilingError> {
        let guard = pprof::ProfilerGuardBuilder::default()
            .frequency(1000)
            .blocklist(&["libc", "libpthread"])
            .build()?;
        
        self.cpu_profiler = Some(guard);
        Ok(())
    }
    
    pub fn stop_cpu_profiling(&mut self) -> Result<Report, ProfilingError> {
        if let Some(guard) = self.cpu_profiler.take() {
            let report = guard.report().build()?;
            Ok(report)
        } else {
            Err(ProfilingError::NotStarted)
        }
    }
    
    pub async fn analyze_hot_paths(&self) -> HotPathAnalysis {
        let traces = self.trace_collector.get_traces().await;
        
        let mut hot_paths = Vec::new();
        let mut function_times = HashMap::new();
        
        for trace in traces {
            for span in trace.spans {
                let duration = span.end_time - span.start_time;
                *function_times.entry(span.name.clone()).or_insert(0) += duration.as_micros();
            }
        }
        
        // Sort by total time
        let mut sorted: Vec<_> = function_times.into_iter().collect();
        sorted.sort_by_key(|k| std::cmp::Reverse(k.1));
        
        for (name, total_micros) in sorted.iter().take(20) {
            hot_paths.push(HotPath {
                function: name.clone(),
                total_time: Duration::from_micros(*total_micros as u64),
                percentage: (*total_micros as f64 / sorted.iter().map(|x| x.1).sum::<u128>() as f64) * 100.0,
            });
        }
        
        HotPathAnalysis {
            hot_paths,
            total_samples: traces.len(),
        }
    }
}

pub struct MemoryTracker {
    snapshots: Arc<RwLock<Vec<MemorySnapshot>>>,
    tracking_enabled: Arc<AtomicBool>,
}

impl MemoryTracker {
    pub fn track_allocations(&self) {
        self.tracking_enabled.store(true, Ordering::SeqCst);
        
        let snapshots = self.snapshots.clone();
        let enabled = self.tracking_enabled.clone();
        
        tokio::spawn(async move {
            while enabled.load(Ordering::SeqCst) {
                let snapshot = MemorySnapshot {
                    timestamp: Instant::now(),
                    heap_size: get_heap_size(),
                    resident_size: get_resident_size(),
                    virtual_size: get_virtual_size(),
                    allocations: get_allocation_count(),
                };
                
                snapshots.write().await.push(snapshot);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    }
    
    pub async fn find_memory_leaks(&self) -> Vec<MemoryLeak> {
        let snapshots = self.snapshots.read().await;
        let mut leaks = Vec::new();
        
        if snapshots.len() < 100 {
            return leaks;
        }
        
        // Analyze growth patterns
        let window_size = 10;
        for window in snapshots.windows(window_size) {
            let start = &window[0];
            let end = &window[window_size - 1];
            
            let growth_rate = (end.heap_size as f64 - start.heap_size as f64) 
                / start.heap_size as f64;
            
            if growth_rate > 0.05 { // 5% growth in window
                leaks.push(MemoryLeak {
                    start_time: start.timestamp,
                    end_time: end.timestamp,
                    growth_bytes: end.heap_size - start.heap_size,
                    growth_rate,
                });
            }
        }
        
        leaks
    }
}
```

2. **Database Query Optimization**
```rust
// src/optimization/database.rs

use sqlx::{Pool, Postgres};
use std::time::Duration;

pub struct DatabaseOptimizer {
    pool: Pool<Postgres>,
    query_stats: Arc<RwLock<QueryStatistics>>,
    cache: Arc<QueryCache>,
}

impl DatabaseOptimizer {
    pub async fn analyze_slow_queries(&self) -> Vec<SlowQuery> {
        let query = r#"
            SELECT 
                query,
                calls,
                total_time,
                mean_time,
                max_time,
                rows
            FROM pg_stat_statements
            WHERE mean_time > 100
            ORDER BY mean_time DESC
            LIMIT 50
        "#;
        
        let rows = sqlx::query_as::<_, SlowQuery>(query)
            .fetch_all(&self.pool)
            .await?;
        
        rows
    }
    
    pub async fn optimize_indexes(&self) -> Result<OptimizationReport, Error> {
        let mut report = OptimizationReport::default();
        
        // Find missing indexes
        let missing = self.find_missing_indexes().await?;
        for index in missing {
            let sql = format!(
                "CREATE INDEX CONCURRENTLY {} ON {} ({})",
                index.name, index.table, index.columns.join(", ")
            );
            
            sqlx::query(&sql).execute(&self.pool).await?;
            report.indexes_created.push(index);
        }
        
        // Find unused indexes
        let unused = self.find_unused_indexes().await?;
        for index in unused {
            let sql = format!("DROP INDEX CONCURRENTLY IF EXISTS {}", index);
            sqlx::query(&sql).execute(&self.pool).await?;
            report.indexes_dropped.push(index);
        }
        
        // Update statistics
        sqlx::query("ANALYZE").execute(&self.pool).await?;
        
        Ok(report)
    }
    
    pub async fn implement_query_cache(&self) {
        // Implement read-through cache for expensive queries
        self.cache.set_policy(CachePolicy {
            max_size: 1000,
            ttl: Duration::from_secs(300),
            eviction: EvictionPolicy::LRU,
        });
        
        // Cache frequently accessed data
        let frequent_queries = vec![
            "SELECT * FROM blocks WHERE height = $1",
            "SELECT * FROM transactions WHERE hash = $1",
            "SELECT * FROM utxos WHERE address = $1 AND spent = false",
        ];
        
        for query in frequent_queries {
            self.cache.register_cacheable(query);
        }
    }
    
    async fn find_missing_indexes(&self) -> Result<Vec<IndexSuggestion>, Error> {
        let query = r#"
            SELECT 
                schemaname,
                tablename,
                attname,
                n_distinct,
                correlation
            FROM pg_stats
            WHERE schemaname = 'public'
                AND n_distinct > 100
                AND correlation < 0.1
                AND NOT EXISTS (
                    SELECT 1 FROM pg_indexes
                    WHERE tablename = pg_stats.tablename
                    AND indexdef LIKE '%' || attname || '%'
                )
        "#;
        
        let rows = sqlx::query_as::<_, MissingIndexRow>(query)
            .fetch_all(&self.pool)
            .await?;
        
        // Convert to index suggestions
        rows.into_iter()
            .map(|row| IndexSuggestion {
                name: format!("idx_{}_{}", row.tablename, row.attname),
                table: row.tablename,
                columns: vec![row.attname],
                estimated_improvement: row.n_distinct as f64 * (1.0 - row.correlation.abs()),
            })
            .collect()
    }
}

pub struct QueryCache {
    cache: Arc<RwLock<HashMap<String, CachedResult>>>,
    policy: CachePolicy,
    stats: Arc<CacheStatistics>,
}

impl QueryCache {
    pub async fn get_or_compute<F, Fut, T>(&self, key: &str, compute: F) -> Result<T, Error>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, Error>>,
        T: Clone + Serialize + DeserializeOwned,
    {
        // Check cache first
        if let Some(cached) = self.get(key).await {
            self.stats.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(cached);
        }
        
        self.stats.misses.fetch_add(1, Ordering::Relaxed);
        
        // Compute result
        let result = compute().await?;
        
        // Store in cache
        self.set(key, &result).await;
        
        Ok(result)
    }
    
    async fn evict_lru(&self) {
        let mut cache = self.cache.write().await;
        
        if cache.len() >= self.policy.max_size {
            // Find least recently used
            let lru_key = cache
                .iter()
                .min_by_key(|(_, v)| v.last_accessed)
                .map(|(k, _)| k.clone());
            
            if let Some(key) = lru_key {
                cache.remove(&key);
                self.stats.evictions.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}
```

3. **Memory Optimization**
```rust
// src/optimization/memory.rs

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct TrackingAllocator {
    allocated: AtomicUsize,
    deallocated: AtomicUsize,
    peak: AtomicUsize,
}

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ret = System.alloc(layout);
        if !ret.is_null() {
            let size = layout.size();
            let allocated = self.allocated.fetch_add(size, Ordering::SeqCst) + size;
            self.peak.fetch_max(allocated, Ordering::SeqCst);
        }
        ret
    }
    
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        self.deallocated.fetch_add(layout.size(), Ordering::SeqCst);
    }
}

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator {
    allocated: AtomicUsize::new(0),
    deallocated: AtomicUsize::new(0),
    peak: AtomicUsize::new(0),
};

pub fn get_memory_stats() -> MemoryStats {
    MemoryStats {
        allocated: ALLOCATOR.allocated.load(Ordering::SeqCst),
        deallocated: ALLOCATOR.deallocated.load(Ordering::SeqCst),
        peak: ALLOCATOR.peak.load(Ordering::SeqCst),
        current: ALLOCATOR.allocated.load(Ordering::SeqCst) 
            - ALLOCATOR.deallocated.load(Ordering::SeqCst),
    }
}

// Object pooling for frequent allocations
pub struct ObjectPool<T> {
    pool: Arc<RwLock<Vec<T>>>,
    factory: Box<dyn Fn() -> T + Send + Sync>,
    max_size: usize,
}

impl<T: Send + 'static> ObjectPool<T> {
    pub fn new<F>(factory: F, max_size: usize) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            pool: Arc::new(RwLock::new(Vec::with_capacity(max_size))),
            factory: Box::new(factory),
            max_size,
        }
    }
    
    pub async fn get(&self) -> PooledObject<T> {
        let mut pool = self.pool.write().await;
        
        let obj = if let Some(obj) = pool.pop() {
            obj
        } else {
            (self.factory)()
        };
        
        PooledObject {
            obj: Some(obj),
            pool: self.pool.clone(),
            max_size: self.max_size,
        }
    }
}

pub struct PooledObject<T> {
    obj: Option<T>,
    pool: Arc<RwLock<Vec<T>>>,
    max_size: usize,
}

impl<T> Drop for PooledObject<T> {
    fn drop(&mut self) {
        if let Some(obj) = self.obj.take() {
            let pool = self.pool.clone();
            let max_size = self.max_size;
            
            tokio::spawn(async move {
                let mut pool = pool.write().await;
                if pool.len() < max_size {
                    pool.push(obj);
                }
            });
        }
    }
}

// Memory-efficient data structures
pub struct CompactTransaction {
    // Use smaller types where possible
    pub hash: [u8; 32],       // Instead of Vec<u8>
    pub block_height: u32,     // Instead of u64
    pub timestamp: u32,        // Unix timestamp instead of DateTime
    pub value: u64,
    pub fee: u32,              // Fees rarely exceed u32 max
    pub input_count: u8,       // Rarely more than 255 inputs
    pub output_count: u8,      // Rarely more than 255 outputs
}

impl From<Transaction> for CompactTransaction {
    fn from(tx: Transaction) -> Self {
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&tx.hash[..32]);
        
        Self {
            hash,
            block_height: tx.block_height as u32,
            timestamp: tx.timestamp.timestamp() as u32,
            value: tx.value,
            fee: tx.fee.min(u32::MAX as u64) as u32,
            input_count: tx.inputs.len().min(255) as u8,
            output_count: tx.outputs.len().min(255) as u8,
        }
    }
}
```

4. **Actor System Optimization**
```rust
// src/optimization/actors.rs

use actix::prelude::*;

pub struct OptimizedActor {
    // Use bounded channels to prevent unbounded growth
    mailbox_limit: usize,
    
    // Batch processing for efficiency
    batch_size: usize,
    batch_timeout: Duration,
    pending_batch: Vec<Message>,
    
    // Message prioritization
    priority_queue: BinaryHeap<PriorityMessage>,
    
    // Backpressure handling
    backpressure_threshold: usize,
    rejection_count: Arc<AtomicU64>,
}

impl Actor for OptimizedActor {
    type Context = Context<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        // Set mailbox capacity
        ctx.set_mailbox_capacity(self.mailbox_limit);
        
        // Start batch processing timer
        ctx.run_interval(self.batch_timeout, |act, _| {
            if !act.pending_batch.is_empty() {
                act.process_batch();
            }
        });
    }
}

impl Handler<Message> for OptimizedActor {
    type Result = ResponseActFuture<Self, Result<(), Error>>;
    
    fn handle(&mut self, msg: Message, ctx: &mut Context<Self>) -> Self::Result {
        // Check backpressure
        if ctx.mailbox_size() > self.backpressure_threshold {
            self.rejection_count.fetch_add(1, Ordering::Relaxed);
            return Box::pin(async { Err(Error::Backpressure) }.into_actor(self));
        }
        
        // Add to batch
        self.pending_batch.push(msg);
        
        // Process if batch is full
        if self.pending_batch.len() >= self.batch_size {
            self.process_batch();
        }
        
        Box::pin(async { Ok(()) }.into_actor(self))
    }
}

impl OptimizedActor {
    fn process_batch(&mut self) {
        let batch = std::mem::take(&mut self.pending_batch);
        
        // Process messages in batch for better cache locality
        for msg in batch {
            self.process_single(msg);
        }
    }
    
    fn process_single(&mut self, msg: Message) {
        // Optimized processing logic
        match msg {
            Message::HighPriority(data) => {
                // Process immediately
                self.handle_high_priority(data);
            }
            Message::LowPriority(data) => {
                // Add to priority queue for deferred processing
                self.priority_queue.push(PriorityMessage {
                    priority: 0,
                    message: data,
                });
            }
            Message::Bulk(items) => {
                // Process in parallel
                items.par_iter().for_each(|item| {
                    self.handle_item(item);
                });
            }
        }
    }
}

// Message coalescing for similar operations
pub struct MessageCoalescer {
    pending: HashMap<MessageKey, Vec<MessageData>>,
    flush_interval: Duration,
}

impl MessageCoalescer {
    pub fn coalesce(&mut self, key: MessageKey, data: MessageData) {
        self.pending.entry(key).or_default().push(data);
    }
    
    pub fn flush(&mut self) -> Vec<CoalescedMessage> {
        self.pending
            .drain()
            .map(|(key, data)| CoalescedMessage { key, data })
            .collect()
    }
}
```

5. **Network Optimization**
```rust
// src/optimization/network.rs

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use bytes::{Bytes, BytesMut};

pub struct OptimizedNetwork {
    // Connection pooling
    connection_pool: Arc<ConnectionPool>,
    
    // Message compression
    compression: CompressionStrategy,
    
    // Protocol buffers for efficient serialization
    use_protobuf: bool,
    
    // TCP tuning parameters
    tcp_nodelay: bool,
    tcp_keepalive: Option<Duration>,
    send_buffer_size: usize,
    recv_buffer_size: usize,
}

impl OptimizedNetwork {
    pub async fn send_optimized(&self, msg: Message) -> Result<(), Error> {
        // Get connection from pool
        let mut conn = self.connection_pool.get().await?;
        
        // Serialize efficiently
        let data = if self.use_protobuf {
            msg.to_protobuf()?
        } else {
            bincode::serialize(&msg)?
        };
        
        // Compress if beneficial
        let compressed = if data.len() > 1024 {
            self.compression.compress(&data)?
        } else {
            data
        };
        
        // Send with zero-copy
        conn.write_all(&compressed).await?;
        
        // Return connection to pool
        self.connection_pool.return_connection(conn).await;
        
        Ok(())
    }
    
    pub async fn batch_send(&self, messages: Vec<Message>) -> Result<(), Error> {
        // Combine multiple messages into single network call
        let mut buffer = BytesMut::with_capacity(messages.len() * 256);
        
        for msg in messages {
            let data = bincode::serialize(&msg)?;
            buffer.extend_from_slice(&(data.len() as u32).to_le_bytes());
            buffer.extend_from_slice(&data);
        }
        
        // Send entire batch
        let mut conn = self.connection_pool.get().await?;
        conn.write_all(&buffer).await?;
        
        Ok(())
    }
}

pub struct ConnectionPool {
    connections: Arc<RwLock<Vec<Connection>>>,
    max_connections: usize,
    min_connections: usize,
    idle_timeout: Duration,
}

impl ConnectionPool {
    pub async fn get(&self) -> Result<PooledConnection, Error> {
        let mut pool = self.connections.write().await;
        
        if let Some(conn) = pool.pop() {
            if conn.is_alive() {
                return Ok(PooledConnection::new(conn, self.connections.clone()));
            }
        }
        
        // Create new connection
        let conn = self.create_connection().await?;
        Ok(PooledConnection::new(conn, self.connections.clone()))
    }
    
    async fn create_connection(&self) -> Result<Connection, Error> {
        let stream = TcpStream::connect(&self.address).await?;
        
        // Apply TCP optimizations
        stream.set_nodelay(true)?;
        stream.set_keepalive(Some(Duration::from_secs(30)))?;
        
        // Set buffer sizes
        let socket = socket2::Socket::from(stream.as_raw_fd());
        socket.set_send_buffer_size(self.send_buffer_size)?;
        socket.set_recv_buffer_size(self.recv_buffer_size)?;
        
        Ok(Connection::new(stream))
    }
}
```

## Testing Plan

### Performance Benchmarks
```rust
#[bench]
fn bench_transaction_processing(b: &mut Bencher) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    
    b.iter(|| {
        runtime.block_on(async {
            let tx = create_large_transaction();
            process_transaction(tx).await
        })
    });
}

#[bench]
fn bench_block_validation(b: &mut Bencher) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    
    b.iter(|| {
        runtime.block_on(async {
            let block = create_full_block();
            validate_block(block).await
        })
    });
}
```

### Memory Leak Detection
```bash
#!/bin/bash
# Run with memory leak detection
RUST_BACKTRACE=1 \
RUSTFLAGS="-Z sanitizer=leak" \
cargo +nightly run --features leak-detection
```

### Load Testing
```yaml
# k6 load test script
import http from 'k6/http';
import { check } from 'k6';

export let options = {
  stages: [
    { duration: '5m', target: 100 },
    { duration: '10m', target: 100 },
    { duration: '5m', target: 200 },
    { duration: '10m', target: 200 },
    { duration: '5m', target: 0 },
  ],
};

export default function() {
  let response = http.post('http://localhost:8545', JSON.stringify({
    jsonrpc: '2.0',
    method: 'eth_sendRawTransaction',
    params: [generateTransaction()],
    id: 1,
  }));
  
  check(response, {
    'status is 200': (r) => r.status === 200,
    'response time < 100ms': (r) => r.timings.duration < 100,
  });
}
```

## Dependencies

### Blockers
- ALYS-016: Production deployment must be stable

### Blocked By
None

### Related Issues
- ALYS-018: Performance monitoring dashboard
- ALYS-019: Capacity planning

## Definition of Done

- [ ] Profiling infrastructure deployed
- [ ] Hot paths identified and optimized
- [ ] Database queries optimized
- [ ] Memory usage reduced by 30%
- [ ] Throughput increased by 50%
- [ ] P99 latency < 100ms
- [ ] Object pooling implemented
- [ ] Network optimizations applied
- [ ] All benchmarks passing

## Notes

- Focus on most impactful optimizations first
- Monitor for regressions after each change
- Document all optimization decisions
- Consider trade-offs between memory and CPU

## Time Tracking

- Estimated: 5 days
- Actual: _To be filled_