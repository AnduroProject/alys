# Alys V2 Development Guide

Complete guide for developing and testing the Alys V2 regtest environment.

---

## Quick Start

### Three Development Modes

| Mode | Command | Build Time | Rebuild Time | Use Case |
|------|---------|------------|--------------|----------|
| **Development** | `docker-compose.v2-regtest.dev.yml` | ~3-5 min | ~30-60 sec | Active development, quick iterations |
| **Hot-Reload** | `docker-compose.v2-regtest.dev-watch.yml` | ~3-5 min | Automatic | Ultra-fast iteration, experimental |
| **Production** | `docker-compose.v2-regtest.yml` | ~8-10 min | ~2-3 min | Final testing, deployments |

### 1. Development Mode (Recommended)
**Fast iteration: ~30-60 seconds per rebuild**

```bash
docker compose -f etc/docker-compose.v2-regtest.dev.yml up -d
docker logs alys-node-1-dev -f
docker logs alys-node-2-dev -f
```

After making code changes:
```bash
docker compose -f docker-compose.v2-regtest.dev.yml restart alys-node-1
```

### 2. Hot-Reload Mode (Experimental)
**Automatic rebuild on file save: ~10-30 seconds**

```bash
cd etc
docker compose -f docker-compose.v2-regtest.dev-watch.yml up
# Save .rs files → automatic rebuild
# Ctrl+C to stop
```

### 3. Production Mode (Final Testing)
**Full build: ~2-3 minutes per rebuild**

```bash
cd etc
docker compose -f docker-compose.v2-regtest.yml up -d --build
docker logs alys-node-1 -f
```

---

## Monitoring (All Modes)

- **Grafana**: http://localhost:3030 (admin/admin)
- **Prometheus**: http://localhost:9092
- **Node 1 Metrics**: http://localhost:9090/metrics
- **Node 2 Metrics**: http://localhost:9091/metrics

---

## Mode 1: Production Build

### Configuration: `docker-compose.v2-regtest.yml`

**How it works:**
- Builds optimized Docker image using multi-stage Dockerfile
- Caches dependencies in Docker BuildKit layers
- Creates minimal runtime container

**When to use:**
- Final testing before deployment
- Performance testing (uses release builds)
- When you need reproducible builds

### Usage

```bash
cd etc

# Build and start
docker compose -f docker-compose.v2-regtest.yml up -d --build

# View logs
docker logs alys-node-1 -f --tail 1000
docker logs alys-node-2 -f --tail 1000
# or
docker compose -f etc/docker-compose.v2-regtest.yml logs alys-node-1 > node1-logs.txt
docker compose -f etc/docker-compose.v2-regtest.yml logs alys-node-2 > node2-logs.txt

# Stop
docker compose -f docker-compose.v2-regtest.yml down
```

### Speed Characteristics

- **First build**: ~8-10 minutes (downloads dependencies)
- **After code changes**: ~2-3 minutes (uses cache)
- **After Cargo.toml changes**: ~3-5 minutes (rebuilds dependencies)

---

## Mode 2: Development Mode (Recommended for Active Development)

### Configuration: `docker-compose.v2-regtest.dev.yml`

**How it works:**
- Uses `rust:bullseye` base image directly (no custom Dockerfile)
- Mounts your source code as a volume (instant code changes)
- Mounts your local Cargo cache (~/.cargo)
- Builds inside the container on startup

**When to use:**
- **Active development** - you're making frequent code changes
- Testing fixes and new features
- Debugging issues
- Quick prototyping

### Usage

```bash
cd etc

# Start development environment
docker compose -f docker-compose.v2-regtest.dev.yml up -d

# View logs from both nodes
docker logs alys-node-1-dev -f
docker logs alys-node-2-dev -f

# After making code changes, restart to rebuild
docker compose -f docker-compose.v2-regtest.dev.yml restart alys-node-1

# Or rebuild and restart both nodes
docker compose -f docker-compose.v2-regtest.dev.yml down
docker compose -f docker-compose.v2-regtest.dev.yml up -d

# Stop everything
docker compose -f docker-compose.v2-regtest.dev.yml down
```

### Testing Your Code Changes

```bash
cd etc

# 1. Start in development mode
docker compose -f docker-compose.v2-regtest.dev.yml up -d

# 2. Watch node 1 logs
docker logs alys-node-1-dev -f

# Expected output:
# ✅ "📦 Initializing StorageActor V2..."
# ✅ "✓ StorageActor V2 started"
# ❌ NO "background task failed" error

# 3. Make code changes in your editor

# 4. Quick restart to apply changes (rebuilds inside container)
docker compose -f docker-compose.v2-regtest.dev.yml restart alys-node-1
```

### Speed Characteristics

- **First startup**: ~3-5 minutes (installs deps, builds code)
- **After code changes**: ~30-60 seconds (incremental rebuild)
- **Startup after restart**: ~30-60 seconds (rebuilds changed files only)

### Advantages

✅ **Faster iteration** - 30-60 second rebuilds vs 2-3 minutes
✅ **Uses local Cargo cache** - shares cache with your local dev environment
✅ **Instant code changes** - edit files in your IDE, restart container
✅ **Debug builds** - faster compilation, includes debug symbols
✅ **Simple setup** - no separate docker build step

---

## Mode 3: Hot-Reload Mode (Experimental)

### Configuration: `docker-compose.v2-regtest.dev-watch.yml`

**How it works:**
- Uses `cargo-watch` to monitor file changes
- Automatically rebuilds and restarts when `.rs` files change
- Keeps logs visible in foreground

**When to use:**
- Rapid prototyping
- UI/UX experimentation
- When you want immediate feedback on code changes
- **NOT recommended for complex debugging** (restarts can be disruptive)

### Usage

```bash
cd etc

# Start hot-reload mode (runs in foreground, shows logs)
docker compose -f docker-compose.v2-regtest.dev-watch.yml up

# In another terminal, make code changes
# Container will automatically rebuild and restart

# Stop with Ctrl+C, then:
docker compose -f docker-compose.v2-regtest.dev-watch.yml down
```

### How It Works

When you save a `.rs` file:
1. **cargo-watch detects** the change (~1 second)
2. **Incremental rebuild** starts (~10-30 seconds)
3. **App restarts** automatically
4. **Logs appear** immediately

### Speed Characteristics

- **First startup**: ~3-5 minutes (installs cargo-watch, builds code)
- **After code changes**: **Automatic** (~10-30 seconds)
- **No manual intervention** needed

### Advantages

✅ **Fully automatic** - save file, wait, see changes
✅ **Fastest feedback loop** - no manual restart needed
✅ **Live logs** - see output immediately

### Disadvantages

⚠️ **Can be disruptive** - restarts interrupt long-running operations
⚠️ **Syntax errors cause failures** - invalid code stops the app
⚠️ **Resource intensive** - cargo-watch uses CPU monitoring files

---

## Comparison Table

### Iteration Speed (Time from Code Change to Running)

| Workflow | Manual Steps | Total Time |
|----------|-------------|------------|
| **Production** | `docker compose up -d --build` | ~2-3 minutes |
| **Development** | `docker compose restart alys-node-1` | ~30-60 seconds |
| **Hot-Reload** | *(save file)* | ~10-30 seconds (automatic) |

### When to Use Each Mode

```
┌─────────────────────────────────────────────────────┐
│                Development Phase                     │
├─────────────────────────────────────────────────────┤
│                                                      │
│  Exploring/Prototyping        → Hot-Reload Mode     │
│  ↓                                                   │
│  Active Development           → Development Mode    │
│  ↓                                                   │
│  Testing/Debugging            → Development Mode    │
│  ↓                                                   │
│  Pre-deployment Validation    → Production Mode     │
│  ↓                                                   │
│  Performance Testing          → Production Mode     │
│                                                      │
└─────────────────────────────────────────────────────┘
```

---

## Shared Features Across All Modes

All three modes include:
- ✅ Two-node regtest network
- ✅ Bitcoin Core (regtest)
- ✅ Reth execution layer
- ✅ Prometheus metrics (port 9092)
- ✅ Grafana dashboard (port 3030)
- ✅ Full P2P networking between nodes

---

## Cleanup

```bash
# Stop development mode
docker compose -f docker-compose.v2-regtest.dev.yml down

# Stop hot-reload mode
docker compose -f docker-compose.v2-regtest.dev-watch.yml down

# Stop production mode
docker compose -f docker-compose.v2-regtest.yml down

# Clean everything (including volumes)
docker compose -f docker-compose.v2-regtest.dev.yml down -v
```

---

## Pro Tips

### Fastest workflow for active development:
1. Use `docker-compose.v2-regtest.dev.yml` (Development Mode)
2. Edit code in your IDE
3. `docker compose -f docker-compose.v2-regtest.dev.yml restart alys-node-1`
4. Check logs: `docker logs alys-node-1-dev -f`
5. Repeat from step 2

### For rapid experimentation:
- Use `docker-compose.v2-regtest.dev-watch.yml` (Hot-Reload Mode)
- Just save files and watch the automatic rebuild

### Before committing:
- Test with `docker-compose.v2-regtest.yml` (Production Mode)
- Ensures release build works correctly

---

## Common Commands

```bash
# View logs from both nodes
docker logs alys-node-1-dev -f & docker logs alys-node-2-dev -f

# Restart just one node
docker compose -f docker-compose.v2-regtest.dev.yml restart alys-node-1

# Rebuild and restart
docker compose -f docker-compose.v2-regtest.dev.yml down
docker compose -f docker-compose.v2-regtest.dev.yml up -d

# Check if ports are available
lsof -i :3000,9090

# See all running containers
docker ps

# Clean up stopped containers
docker compose -f docker-compose.v2-regtest.dev.yml down
```

---

## Troubleshooting

### Development mode won't start

**Problem**: Container exits immediately

**Solution**: Check if ports are already in use
```bash
# Check what's using the ports
lsof -i :3000
lsof -i :9090

# Stop conflicting containers
docker compose -f docker-compose.v2-regtest.yml down
```

### Cargo cache not working

**Problem**: Rebuilds are slow even in development mode

**Solution**: Ensure local cargo cache exists
```bash
# Check cache directory
ls -la ~/.cargo/registry
ls -la ~/.cargo/git

# If empty, cargo will populate it on first build
```

### Hot-reload not triggering

**Problem**: cargo-watch doesn't detect changes

**Solution**:
1. Check file permissions (mounted volumes)
2. Ensure you're editing files in the mounted directory
3. Try manually triggering: `touch app/src/main.rs`

### Container logs show "permission denied"

**Problem**: Volume mount permission issues

**Solution**:
```bash
# Fix data directory permissions
chmod -R 755 data/node1
chmod -R 755 data/node2
```

### Exit code 100 errors

**Problem**: Container exits with code 100

**Possible causes**:
- Bash script syntax errors in docker-compose file
- Missing system dependencies
- Path or volume mount issues
- Cargo build failure

**Solution**: Check container logs for specific errors
```bash
docker logs alys-node-1-dev 2>&1 | tail -50
```

---

## Advanced: Hybrid Workflow

You can combine modes for maximum efficiency:

### Use Local Cargo Build + Volume Mount

This is the absolute fastest approach:

```bash
# 1. Build locally (uses your native CPU, local cache)
cd /Users/michael/zDevelopment/Mara/alys-v2
cargo build --bin app

# 2. Start container with volume-mounted binary
# (requires modifying docker-compose.v2-regtest.dev.yml to mount ./target/debug/app)

# This gives you:
# - Local build speed (~20-30 seconds)
# - Container environment isolation
# - Instant restart (no rebuild in container)
```

To set this up, add to `docker-compose.v2-regtest.dev.yml`:
```yaml
volumes:
  - ./target/debug/app:/bin/alys:ro
command: ["/bin/alys", "--dev-regtest", "..."]
```

---

## Recommended Workflow for Development

**For your current task (testing changes):**

```bash
# Use Development Mode
cd etc
docker compose -f docker-compose.v2-regtest.dev.yml up -d
docker logs alys-node-1-dev -f
```

**Speed: 30-60 seconds per iteration**

This gives you the best balance of:
- ✅ Fast rebuilds
- ✅ Full environment (both nodes, metrics, etc.)
- ✅ Easy debugging (full logs, debug symbols)
- ✅ Reproducible (same environment as production)

---

## Summary

**Quick Reference:**

```bash
# Development (Fast iteration - 30-60 sec rebuilds)
docker compose -f docker-compose.v2-regtest.dev.yml up -d
docker compose -f docker-compose.v2-regtest.dev.yml restart alys-node-1

# Hot-Reload (Automatic - 10-30 sec, foreground)
docker compose -f docker-compose.v2-regtest.dev-watch.yml up

# Production (Final testing - 2-3 min rebuilds)
docker compose -f docker-compose.v2-regtest.yml up -d --build
```

Choose the mode that matches your development phase for optimal productivity!
