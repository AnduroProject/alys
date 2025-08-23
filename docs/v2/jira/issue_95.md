# ALYS-005: Setup CI/CD Pipeline with Migration Support

## Issue Type
Task

## Priority
High

## Sprint
Migration Sprint 1

## Component
DevOps

## Labels
`alys`, `v2`, `devops`

## Description

Establish a comprehensive CI/CD pipeline that supports the migration process with automated testing, gradual rollouts, rollback capabilities, and integration with feature flags. The pipeline should ensure safe and reliable deployments throughout the migration phases.

## Acceptance Criteria

## Detailed Implementation Subtasks (22 tasks across 7 phases)

### Phase 1: Core CI Workflows (4 tasks)
- [ ] **ALYS-005-01**: Create main CI workflow with linting, formatting, clippy, and documentation checks
- [ ] **ALYS-005-02**: Implement comprehensive testing pipeline with unit, integration, and property-based tests
- [ ] **ALYS-005-03**: Set up code coverage tracking with tarpaulin and 80% threshold enforcement
- [ ] **ALYS-005-04**: Create build workflow with multi-target compilation (x86_64, aarch64) and artifact upload

### Phase 2: Security & Quality (3 tasks)
- [ ] **ALYS-005-05**: Implement security scanning with cargo-audit, cargo-deny, and Semgrep SAST
- [ ] **ALYS-005-06**: Add dependency vulnerability scanning with automated alerts
- [ ] **ALYS-005-07**: Create security policy enforcement with license checking and deny lists

### Phase 3: Migration-Specific Testing (3 tasks)
- [ ] **ALYS-005-08**: Create migration phase testing workflow with backup/restore capabilities
- [ ] **ALYS-005-09**: Implement migration validation scripts for each phase with rollback testing
- [ ] **ALYS-005-10**: Add migration gate checks with metrics validation and error rate thresholds

### Phase 4: Docker & Registry (3 tasks)
- [ ] **ALYS-005-11**: Set up Docker multi-platform builds with cache optimization and metadata extraction
- [ ] **ALYS-005-12**: Implement container registry push to GitHub Container Registry with tagging strategy
- [ ] **ALYS-005-13**: Add container security scanning and vulnerability assessment

### Phase 5: Deployment Automation (4 tasks)
- [ ] **ALYS-005-14**: Create deployment workflow with environment-specific configurations and approval gates
- [ ] **ALYS-005-15**: Implement Helm-based Kubernetes deployments with rollout percentage control
- [ ] **ALYS-005-16**: Add smoke testing and deployment validation with automated health checks
- [ ] **ALYS-005-17**: Create deployment status tracking with GitHub deployments API integration

### Phase 6: Rollback & Recovery (3 tasks)
- [ ] **ALYS-005-18**: Implement automated rollback workflow with version detection and Helm rollback
- [ ] **ALYS-005-19**: Add rollback verification with deployment testing and status validation
- [ ] **ALYS-005-20**: Create emergency rollback procedures with manual trigger and fast execution

### Phase 7: Performance & Monitoring (2 tasks)
- [ ] **ALYS-005-21**: Set up performance regression detection with benchmarking and alert thresholds
- [ ] **ALYS-005-22**: Implement notification system with Slack integration and deployment status updates

## Original Acceptance Criteria
- [ ] GitHub Actions workflows configured for all branches
- [ ] Automated testing pipeline (unit, integration, e2e)
- [ ] Docker image building and registry push
- [ ] Deployment automation for test/staging/production
- [ ] Rollback automation implemented
- [ ] Feature flag integration in deployment process
- [ ] Performance regression detection
- [ ] Security scanning (SAST, dependency scanning)
- [ ] Deployment notifications to Slack/Discord

## Technical Details

### Implementation Steps

1. **Main CI Workflow**
```yaml
# .github/workflows/ci.yml

name: Continuous Integration

on:
  push:
    branches: [main, develop, 'release/*', 'migration/*']
  pull_request:
    branches: [main, develop]

env:
  RUST_VERSION: 1.75.0
  CARGO_TERM_COLOR: always
  RUSTFLAGS: "-D warnings"

jobs:
  lint:
    name: Lint
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: ${{ env.RUST_VERSION }}
          components: rustfmt, clippy
      
      - name: Cache cargo
        uses: actions/cache@v3
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
      
      - name: Check formatting
        run: cargo fmt --all -- --check
      
      - name: Run clippy
        run: cargo clippy --all-targets --all-features -- -D warnings
      
      - name: Check documentation
        run: cargo doc --no-deps --document-private-items --all-features

  test:
    name: Test
    runs-on: ubuntu-latest
    strategy:
      matrix:
        test-type: [unit, integration, property]
    services:
      postgres:
        image: postgres:14
        env:
          POSTGRES_PASSWORD: test
          POSTGRES_DB: alys_test
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
        ports:
          - 5432:5432
    steps:
      - uses: actions/checkout@v4
      
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: ${{ env.RUST_VERSION }}
      
      - name: Install test dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y libssl-dev pkg-config
      
      - name: Run ${{ matrix.test-type }} tests
        run: |
          case "${{ matrix.test-type }}" in
            unit)
              cargo test --lib --bins
              ;;
            integration)
              cargo test --test '*' --features integration
              ;;
            property)
              PROPTEST_CASES=1000 cargo test --test property_tests
              ;;
          esac
      
      - name: Upload test results
        if: always()
        uses: actions/upload-artifact@v3
        with:
          name: test-results-${{ matrix.test-type }}
          path: target/test-results/

  coverage:
    name: Code Coverage
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: ${{ env.RUST_VERSION }}
      
      - name: Install tarpaulin
        run: cargo install cargo-tarpaulin
      
      - name: Generate coverage
        run: cargo tarpaulin --out Xml --all-features --workspace
      
      - name: Upload coverage to Codecov
        uses: codecov/codecov-action@v3
        with:
          files: ./cobertura.xml
          fail_ci_if_error: true
      
      - name: Check coverage threshold
        run: |
          COVERAGE=$(cargo tarpaulin --print-summary | grep "Coverage" | awk '{print $2}' | sed 's/%//')
          if (( $(echo "$COVERAGE < 80" | bc -l) )); then
            echo "Coverage $COVERAGE% is below threshold of 80%"
            exit 1
          fi

  security:
    name: Security Scan
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Run cargo audit
        uses: actions-rs/audit-check@v1
        with:
          token: ${{ secrets.GITHUB_TOKEN }}
      
      - name: Run cargo deny
        uses: EmbarkStudios/cargo-deny-action@v1
      
      - name: SAST with Semgrep
        uses: returntocorp/semgrep-action@v1
        with:
          config: auto

  build:
    name: Build
    needs: [lint, test]
    runs-on: ubuntu-latest
    strategy:
      matrix:
        target: [x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu]
    steps:
      - uses: actions/checkout@v4
      
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: ${{ env.RUST_VERSION }}
          targets: ${{ matrix.target }}
      
      - name: Build release
        run: cargo build --release --target ${{ matrix.target }}
      
      - name: Upload artifacts
        uses: actions/upload-artifact@v3
        with:
          name: alys-${{ matrix.target }}
          path: target/${{ matrix.target }}/release/alys
```

2. **Migration Testing Workflow**
```yaml
# .github/workflows/migration-test.yml

name: Migration Testing

on:
  workflow_dispatch:
    inputs:
      migration_phase:
        description: 'Migration phase to test'
        required: true
        type: choice
        options:
          - foundation
          - actor-core
          - sync-improvement
          - lighthouse-migration
          - governance-integration
          - complete

jobs:
  migration-test:
    name: Test Migration Phase
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Setup test environment
        run: |
          docker-compose -f docker-compose.test.yml up -d
          ./scripts/wait-for-services.sh
      
      - name: Backup current state
        run: |
          ./scripts/backup_system.sh
          echo "BACKUP_DIR=$(ls -t /var/backups/alys | head -1)" >> $GITHUB_ENV
      
      - name: Run migration phase test
        run: |
          cargo test --test migration_${{ github.event.inputs.migration_phase }}_test \
            --features migration-test \
            -- --test-threads=1 --nocapture
      
      - name: Validate migration
        run: |
          ./tests/migration/validate_${{ github.event.inputs.migration_phase }}.sh
      
      - name: Test rollback
        if: github.event.inputs.migration_phase != 'foundation'
        run: |
          ./scripts/restore_system.sh /var/backups/alys/${{ env.BACKUP_DIR }}
          ./tests/migration/validate_rollback.sh
      
      - name: Generate report
        if: always()
        run: |
          ./scripts/generate_migration_report.sh ${{ github.event.inputs.migration_phase }}
      
      - name: Upload report
        if: always()
        uses: actions/upload-artifact@v3
        with:
          name: migration-report-${{ github.event.inputs.migration_phase }}
          path: reports/migration/
```

3. **Docker Build and Push Workflow**
```yaml
# .github/workflows/docker.yml

name: Docker Build and Push

on:
  push:
    branches: [main, develop]
    tags: ['v*']
  workflow_dispatch:

env:
  REGISTRY: ghcr.io
  IMAGE_NAME: ${{ github.repository }}

jobs:
  build-and-push:
    name: Build and Push Docker Image
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: write
    steps:
      - uses: actions/checkout@v4
      
      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v3
      
      - name: Log in to Container Registry
        uses: docker/login-action@v3
        with:
          registry: ${{ env.REGISTRY }}
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}
      
      - name: Extract metadata
        id: meta
        uses: docker/metadata-action@v5
        with:
          images: ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}
          tags: |
            type=ref,event=branch
            type=ref,event=pr
            type=semver,pattern={{version}}
            type=semver,pattern={{major}}.{{minor}}
            type=sha,prefix={{branch}}-
            type=raw,value=migration-{{date 'YYYYMMDD'}}-{{sha}}
      
      - name: Build and push Docker image
        uses: docker/build-push-action@v5
        with:
          context: .
          platforms: linux/amd64,linux/arm64
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
          build-args: |
            FEATURES=${{ contains(github.ref, 'migration') && 'migration' || 'default' }}
```

4. **Deployment Workflow**
```yaml
# .github/workflows/deploy.yml

name: Deploy

on:
  workflow_dispatch:
    inputs:
      environment:
        description: 'Environment to deploy to'
        required: true
        type: choice
        options:
          - testnet
          - staging
          - canary
          - production
      version:
        description: 'Version to deploy'
        required: true
      rollout_percentage:
        description: 'Rollout percentage (for canary/production)'
        required: false
        default: '10'

jobs:
  pre-deployment:
    name: Pre-deployment Checks
    runs-on: ubuntu-latest
    outputs:
      proceed: ${{ steps.checks.outputs.proceed }}
    steps:
      - name: Check deployment conditions
        id: checks
        run: |
          # Check if previous deployment succeeded
          LAST_DEPLOYMENT=$(gh api /repos/${{ github.repository }}/deployments \
            --jq '.[] | select(.environment == "${{ github.event.inputs.environment }}") | .id' \
            | head -1)
          
          if [ -n "$LAST_DEPLOYMENT" ]; then
            STATUS=$(gh api /repos/${{ github.repository }}/deployments/$LAST_DEPLOYMENT/statuses \
              --jq '.[0].state')
            if [ "$STATUS" != "success" ]; then
              echo "Last deployment did not succeed: $STATUS"
              echo "proceed=false" >> $GITHUB_OUTPUT
              exit 0
            fi
          fi
          
          echo "proceed=true" >> $GITHUB_OUTPUT
      
      - name: Notify deployment start
        if: steps.checks.outputs.proceed == 'true'
        uses: 8398a7/action-slack@v3
        with:
          status: custom
          custom_payload: |
            {
              text: "🚀 Deployment started",
              attachments: [{
                color: "warning",
                fields: [
                  { title: "Environment", value: "${{ github.event.inputs.environment }}", short: true },
                  { title: "Version", value: "${{ github.event.inputs.version }}", short: true },
                  { title: "Triggered by", value: "${{ github.actor }}", short: true }
                ]
              }]
            }
          webhook_url: ${{ secrets.SLACK_WEBHOOK }}

  deploy:
    name: Deploy to ${{ github.event.inputs.environment }}
    needs: pre-deployment
    if: needs.pre-deployment.outputs.proceed == 'true'
    runs-on: ubuntu-latest
    environment: ${{ github.event.inputs.environment }}
    steps:
      - uses: actions/checkout@v4
      
      - name: Setup kubectl
        uses: azure/setup-kubectl@v3
      
      - name: Configure AWS credentials
        uses: aws-actions/configure-aws-credentials@v4
        with:
          role-to-assume: ${{ secrets.AWS_ROLE_ARN }}
          aws-region: us-east-1
      
      - name: Update kubeconfig
        run: |
          aws eks update-kubeconfig --name alys-${{ github.event.inputs.environment }}
      
      - name: Update feature flags
        run: |
          kubectl create configmap feature-flags \
            --from-file=config/features-${{ github.event.inputs.environment }}.toml \
            --dry-run=client -o yaml | kubectl apply -f -
      
      - name: Deploy with Helm
        run: |
          helm upgrade --install alys ./helm/alys \
            --namespace alys \
            --create-namespace \
            --set image.tag=${{ github.event.inputs.version }} \
            --set environment=${{ github.event.inputs.environment }} \
            --set rollout.percentage=${{ github.event.inputs.rollout_percentage }} \
            --wait \
            --timeout 10m
      
      - name: Run smoke tests
        run: |
          kubectl run smoke-test \
            --image=ghcr.io/${{ github.repository }}/test:${{ github.event.inputs.version }} \
            --restart=Never \
            --command -- /tests/smoke_test.sh
          
          kubectl wait --for=condition=Succeeded pod/smoke-test --timeout=5m
      
      - name: Update deployment status
        if: always()
        uses: actions/github-script@v7
        with:
          script: |
            const deployment = await github.rest.repos.createDeployment({
              owner: context.repo.owner,
              repo: context.repo.repo,
              ref: '${{ github.event.inputs.version }}',
              environment: '${{ github.event.inputs.environment }}',
              required_contexts: [],
              auto_merge: false
            });
            
            await github.rest.repos.createDeploymentStatus({
              owner: context.repo.owner,
              repo: context.repo.repo,
              deployment_id: deployment.data.id,
              state: '${{ job.status }}',
              environment_url: 'https://${{ github.event.inputs.environment }}.alys.network',
              description: 'Deployment ${{ job.status }}'
            });
```

5. **Rollback Workflow**
```yaml
# .github/workflows/rollback.yml

name: Rollback Deployment

on:
  workflow_dispatch:
    inputs:
      environment:
        description: 'Environment to rollback'
        required: true
        type: choice
        options:
          - testnet
          - staging
          - canary
          - production
      target_version:
        description: 'Version to rollback to (leave empty for previous)'
        required: false

jobs:
  rollback:
    name: Rollback ${{ github.event.inputs.environment }}
    runs-on: ubuntu-latest
    environment: ${{ github.event.inputs.environment }}-rollback
    steps:
      - uses: actions/checkout@v4
      
      - name: Get rollback version
        id: version
        run: |
          if [ -n "${{ github.event.inputs.target_version }}" ]; then
            VERSION="${{ github.event.inputs.target_version }}"
          else
            # Get previous successful deployment
            VERSION=$(helm history alys -n alys --max 10 -o json \
              | jq -r '.[] | select(.status == "deployed") | .app_version' \
              | head -2 | tail -1)
          fi
          echo "version=$VERSION" >> $GITHUB_OUTPUT
      
      - name: Rollback with Helm
        run: |
          helm rollback alys -n alys --wait --timeout 10m
      
      - name: Verify rollback
        run: |
          kubectl rollout status deployment/alys -n alys
          ./tests/verify_deployment.sh ${{ github.event.inputs.environment }}
      
      - name: Notify rollback
        if: always()
        uses: 8398a7/action-slack@v3
        with:
          status: ${{ job.status }}
          custom_payload: |
            {
              text: "⏪ Rollback ${{ job.status }}",
              attachments: [{
                color: "${{ job.status == 'success' && 'good' || 'danger' }}",
                fields: [
                  { title: "Environment", value: "${{ github.event.inputs.environment }}", short: true },
                  { title: "Rolled back to", value: "${{ steps.version.outputs.version }}", short: true }
                ]
              }]
            }
          webhook_url: ${{ secrets.SLACK_WEBHOOK }}
```

6. **Performance Regression Detection**
```yaml
# .github/workflows/performance.yml

name: Performance Tests

on:
  pull_request:
    paths:
      - 'src/**'
      - 'Cargo.toml'
  schedule:
    - cron: '0 0 * * *'  # Daily

jobs:
  benchmark:
    name: Performance Benchmarks
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
      
      - name: Run benchmarks
        run: |
          cargo bench --features bench -- --output-format bencher | tee output.txt
      
      - name: Store benchmark result
        uses: benchmark-action/github-action-benchmark@v1
        with:
          tool: 'cargo'
          output-file-path: output.txt
          github-token: ${{ secrets.GITHUB_TOKEN }}
          auto-push: true
          alert-threshold: '110%'
          comment-on-alert: true
          fail-on-alert: true
          alert-comment-cc-users: '@performance-team'
```

7. **Migration Phase Gate Script**
```bash
#!/bin/bash
# scripts/ci/migration_gate.sh

set -euo pipefail

PHASE=$1
METRICS_ENDPOINT="http://localhost:9090/api/v1/query"

check_phase_metrics() {
    local phase=$1
    
    # Check error rate
    ERROR_RATE=$(curl -s "${METRICS_ENDPOINT}?query=rate(alys_migration_errors_total[5m])" \
        | jq -r '.data.result[0].value[1]')
    
    if (( $(echo "$ERROR_RATE > 0.01" | bc -l) )); then
        echo "Error rate too high: $ERROR_RATE"
        return 1
    fi
    
    # Check rollback count
    ROLLBACKS=$(curl -s "${METRICS_ENDPOINT}?query=alys_migration_rollbacks_total" \
        | jq -r '.data.result[0].value[1]')
    
    if [ "$ROLLBACKS" -gt 0 ]; then
        echo "Rollbacks detected: $ROLLBACKS"
        return 1
    fi
    
    # Phase-specific checks
    case "$phase" in
        actor-core)
            check_actor_metrics
            ;;
        sync-improvement)
            check_sync_metrics
            ;;
        lighthouse-migration)
            check_lighthouse_metrics
            ;;
        governance-integration)
            check_governance_metrics
            ;;
    esac
}

check_actor_metrics() {
    # Check actor restart rate
    RESTART_RATE=$(curl -s "${METRICS_ENDPOINT}?query=rate(alys_actor_restarts_total[5m])" \
        | jq -r '.data.result[0].value[1]')
    
    if (( $(echo "$RESTART_RATE > 0.1" | bc -l) )); then
        echo "Actor restart rate too high: $RESTART_RATE"
        return 1
    fi
}

check_sync_metrics() {
    # Check sync progress
    SYNC_PROGRESS=$(curl -s "${METRICS_ENDPOINT}?query=alys_sync_blocks_per_second" \
        | jq -r '.data.result[0].value[1]')
    
    if (( $(echo "$SYNC_PROGRESS < 100" | bc -l) )); then
        echo "Sync too slow: $SYNC_PROGRESS blocks/sec"
        return 1
    fi
}

# Run checks
if check_phase_metrics "$PHASE"; then
    echo "✅ Phase $PHASE gate checks passed"
    exit 0
else
    echo "❌ Phase $PHASE gate checks failed"
    exit 1
fi
```

## Testing Plan

### Unit Tests
- Test individual CI/CD components
- Validate deployment scripts
- Test rollback procedures

### Integration Tests
```bash
# Test full deployment pipeline
./tests/ci/test_deployment_pipeline.sh

# Test rollback
./tests/ci/test_rollback.sh

# Test feature flag integration
./tests/ci/test_feature_flags.sh
```

### End-to-End Tests
1. Deploy to test environment
2. Run smoke tests
3. Trigger rollback
4. Verify rollback succeeded

## Dependencies

### Blockers
None

### Blocked By
- ALYS-001: Backup system for rollback testing
- ALYS-003: Metrics for deployment validation
- ALYS-004: Feature flags for gradual rollout

### Related Issues
- All migration phase tickets depend on CI/CD

## Definition of Done

- [ ] All workflows created and tested
- [ ] Deployment automation working
- [ ] Rollback procedures validated
- [ ] Performance regression detection operational
- [ ] Security scanning integrated
- [ ] Notifications configured
- [ ] Documentation complete
- [ ] Runbook for CI/CD operations

## Notes

- Consider using Argo CD for GitOps
- Implement blue-green deployments for zero downtime
- Add cost monitoring for cloud resources
- Consider using Flux for Kubernetes deployments

## Time Tracking

**Time Estimate**: 3-4 days (24-32 hours total) with detailed breakdown:
- Phase 1 - Core CI workflows: 6-8 hours (includes GitHub Actions setup, testing matrix, coverage integration)
- Phase 2 - Security & quality: 3-4 hours (includes SAST integration, dependency scanning, policy enforcement)
- Phase 3 - Migration-specific testing: 4-5 hours (includes phase testing, validation scripts, gate checks)
- Phase 4 - Docker & registry: 3-4 hours (includes multi-platform builds, registry push, security scanning)
- Phase 5 - Deployment automation: 6-8 hours (includes Kubernetes deployment, Helm charts, smoke testing)
- Phase 6 - Rollback & recovery: 3-4 hours (includes rollback workflows, verification, emergency procedures)
- Phase 7 - Performance & monitoring: 2-3 hours (includes benchmarking, notifications, monitoring integration)

**Critical Path Dependencies**: Phase 1 → Phase 2 → (Phase 3,4 in parallel) → Phase 5 → Phase 6 → Phase 7
**Resource Requirements**: 1 DevOps engineer with GitHub Actions and Kubernetes experience
**Risk Buffer**: 30% additional time for Kubernetes configuration and security policy setup
**Prerequisites**: ALYS-001 (backup system), ALYS-003 (metrics), ALYS-004 (feature flags)
**External Dependencies**: AWS EKS cluster, Slack webhooks, GitHub Container Registry access

- Actual: _To be filled_