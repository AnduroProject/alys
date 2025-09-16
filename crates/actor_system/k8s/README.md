# Alys V2 Actor System - Kubernetes Test Environment

This directory contains Kubernetes manifests and configurations for running comprehensive tests of the Alys V2 actor system in a containerized environment.

## Overview

The Kubernetes test environment provides:
- **Isolated Testing Namespace**: All resources run in `alys-v2-testing` namespace
- **Mock Services**: Simulated external dependencies (governance nodes, Bitcoin/Ethereum nodes)
- **Monitoring Stack**: Prometheus and Grafana for metrics collection and visualization
- **Test Runner**: Containerized test execution with different test types
- **Automated Testing**: Scheduled regression tests and CI/CD integration

## Components

### Core Infrastructure
- **Namespace**: `alys-v2-testing` - Isolated environment for testing
- **ConfigMaps**: Test configuration and service endpoints
- **Secrets**: Test keys and credentials
- **ServiceAccount & RBAC**: Permissions for test runner operations

### Test Runner
- **Deployment**: Main test runner with health checks and metrics
- **Service**: Internal communication and monitoring endpoints
- **Jobs**: Individual test execution (integration, supervision, performance)
- **CronJob**: Nightly regression testing

### Mock Services
- **Governance Nodes**: 3 mock governance nodes with gRPC endpoints
- **Bitcoin Node**: Mock Bitcoin regtest node
- **Ethereum Node**: Mock Ethereum development node

### Monitoring
- **Prometheus**: Metrics collection and storage
- **Grafana**: Dashboard and visualization
- **Custom Dashboards**: Actor system specific metrics

## Quick Start

### Prerequisites
- Kubernetes cluster (v1.20+)
- kubectl configured
- Docker for building images

### 1. Deploy Base Infrastructure
```bash
# Create namespace and basic resources
kubectl apply -f namespace.yaml

# Deploy mock services
kubectl apply -f mock-services.yaml

# Deploy monitoring stack
kubectl apply -f monitoring.yaml
```

### 2. Build and Deploy Test Runner
```bash
# Build test runner image
docker build -f Dockerfile.test-runner -t alys-v2-test-runner:latest ../../../

# Tag and push to your registry
docker tag alys-v2-test-runner:latest your-registry/alys-v2-test-runner:latest
docker push your-registry/alys-v2-test-runner:latest

# Update image reference in test-deployment.yaml
# Deploy test runner
kubectl apply -f test-deployment.yaml
```

### 3. Run Tests
```bash
# Run integration tests
kubectl apply -f test-jobs.yaml

# Check test progress
kubectl logs -f job/integration-test-job -n alys-v2-testing

# Run specific test types
kubectl create job --from=cronjob/nightly-regression-tests manual-regression-test -n alys-v2-testing
```

## Test Types

### Integration Tests
- **Purpose**: Test cross-actor communication and coordination
- **Scenarios**: Block production, bridge operations, multi-actor flows
- **Duration**: ~3-5 minutes
- **Resource Requirements**: 1Gi memory, 500m CPU

### Supervision Tests  
- **Purpose**: Test actor supervision trees and failure handling
- **Scenarios**: Actor failures, cascading failures, recovery patterns
- **Duration**: ~2-3 minutes
- **Resource Requirements**: 512Mi memory, 300m CPU

### Performance Tests
- **Purpose**: Validate system performance under load
- **Metrics**: Message throughput, latency, memory usage
- **Duration**: ~5-10 minutes
- **Resource Requirements**: 2Gi memory, 1000m CPU

### Regression Tests
- **Purpose**: Comprehensive testing for CI/CD
- **Schedule**: Nightly at 2 AM
- **Coverage**: All test types with extended scenarios
- **Resource Requirements**: 4Gi memory, 2000m CPU

## Monitoring and Observability

### Prometheus Metrics
Access metrics at: `http://prometheus:9090` (within cluster)

Key metrics:
- `alys_active_actors` - Number of active actors
- `alys_messages_processed_total` - Message processing rate  
- `alys_system_health_score` - Overall system health
- `alys_actor_restarts_total` - Actor restart count
- `alys_memory_usage_bytes` - Memory usage per actor

### Grafana Dashboards
Access dashboards at: `http://grafana:3000` (admin/admin)

Available dashboards:
- **Actor System Overview**: High-level system metrics
- **Performance Monitoring**: Throughput and latency
- **Error Analysis**: Failure rates and error patterns
- **Resource Utilization**: Memory and CPU usage

### Logs
```bash
# Test runner logs
kubectl logs deployment/alys-v2-test-runner -n alys-v2-testing -f

# Mock service logs  
kubectl logs deployment/mock-governance-1 -n alys-v2-testing

# Job execution logs
kubectl logs job/integration-test-job -n alys-v2-testing
```

## Configuration

### Test Configuration
Edit `test-config.toml` to customize:
- Test timeouts and concurrency
- Mock service endpoints
- Performance thresholds
- Monitoring settings

### Environment Variables
Key environment variables in deployments:
- `TEST_ENVIRONMENT=k8s` - Enables Kubernetes-specific features
- `GOVERNANCE_ENDPOINTS` - List of mock governance endpoints
- `PROMETHEUS_ENABLED=true` - Enable metrics collection
- `RUST_LOG=debug` - Logging level

### Resource Limits
Adjust resource limits in manifests based on cluster capacity:
```yaml
resources:
  requests:
    memory: "512Mi"
    cpu: "500m"
  limits:
    memory: "2Gi" 
    cpu: "2000m"
```

## CI/CD Integration

### GitHub Actions Example
```yaml
name: Kubernetes Tests
on: [push, pull_request]
jobs:
  k8s-tests:
    runs-on: ubuntu-latest
    steps:
    - name: Deploy to test cluster
      run: |
        kubectl apply -f k8s/
        kubectl wait --for=condition=ready pod -l app=alys-v2-test-runner -n alys-v2-testing --timeout=300s
        
    - name: Run integration tests
      run: |
        kubectl apply -f k8s/test-jobs.yaml
        kubectl wait --for=condition=complete job/integration-test-job -n alys-v2-testing --timeout=600s
        
    - name: Collect results
      run: |
        kubectl logs job/integration-test-job -n alys-v2-testing > test-results.log
```

### Jenkins Pipeline Example
```groovy
pipeline {
    agent any
    stages {
        stage('Deploy Test Environment') {
            steps {
                sh 'kubectl apply -f k8s/'
            }
        }
        stage('Run Tests') {
            parallel {
                stage('Integration Tests') {
                    steps {
                        sh 'kubectl create job --from=job/integration-test-job integration-${BUILD_NUMBER}'
                    }
                }
                stage('Performance Tests') {
                    steps {
                        sh 'kubectl create job --from=job/performance-test-job performance-${BUILD_NUMBER}'
                    }
                }
            }
        }
        stage('Collect Results') {
            steps {
                sh 'kubectl logs job/integration-${BUILD_NUMBER} > integration-results.log'
                publishHTML([allowMissing: false, alwaysLinkToLastBuild: true, keepAll: true, reportDir: '.', reportFiles: 'integration-results.log', reportName: 'Integration Test Report'])
            }
        }
    }
    post {
        always {
            sh 'kubectl delete namespace alys-v2-testing --ignore-not-found=true'
        }
    }
}
```

## Troubleshooting

### Common Issues

**Test runner not starting**
```bash
# Check pod status
kubectl describe pod -l app=alys-v2-test-runner -n alys-v2-testing

# Check logs
kubectl logs deployment/alys-v2-test-runner -n alys-v2-testing
```

**Mock services unreachable**
```bash
# Verify service endpoints
kubectl get svc -n alys-v2-testing

# Test connectivity
kubectl run debug --rm -i --tty --image=busybox -- nslookup mock-governance-1.alys-v2-testing.svc.cluster.local
```

**Tests timing out**
```bash
# Check resource constraints
kubectl top pods -n alys-v2-testing

# Increase timeouts in test-config.toml
# Scale up resources in deployments
```

**Prometheus not scraping metrics**
```bash
# Check service discovery
kubectl logs deployment/prometheus -n alys-v2-testing

# Verify annotations on test runner service
kubectl describe svc alys-v2-test-runner-service -n alys-v2-testing
```

### Debug Mode
Enable verbose logging:
```bash
kubectl set env deployment/alys-v2-test-runner RUST_LOG=trace -n alys-v2-testing
```

### Resource Monitoring
```bash
# Check resource usage
kubectl top pods -n alys-v2-testing
kubectl top nodes

# Monitor in real-time
watch kubectl get pods -n alys-v2-testing
```

## Cleanup

### Manual Cleanup
```bash
# Delete all test resources
kubectl delete namespace alys-v2-testing

# Or delete specific components
kubectl delete -f test-jobs.yaml
kubectl delete -f test-deployment.yaml
kubectl delete -f mock-services.yaml
kubectl delete -f monitoring.yaml
kubectl delete -f namespace.yaml
```

### Automated Cleanup
Jobs automatically clean up after completion based on `ttlSecondsAfterFinished` setting. Failed jobs are preserved for debugging.

## Security Considerations

- **Network Policies**: Restrict pod-to-pod communication
- **Resource Quotas**: Prevent resource exhaustion
- **Secret Management**: Use proper secret management for production
- **RBAC**: Minimal permissions for service accounts
- **Image Security**: Scan images for vulnerabilities

## Production Adaptations

For production-like testing:
1. Use persistent volumes for logs and metrics
2. Implement proper monitoring and alerting
3. Add network policies for isolation
4. Use Helm charts for easier deployment
5. Integrate with external monitoring systems
6. Implement proper backup and disaster recovery