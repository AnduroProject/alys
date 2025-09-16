#!/bin/bash

# Health check script for Alys V2 Test Runner

set -e

# Check if the test runner process is responding
if ! curl -f -s http://localhost:8080/health > /dev/null 2>&1; then
    echo "Health endpoint not responding"
    exit 1
fi

# Check if metrics endpoint is available
if ! curl -f -s http://localhost:9090/metrics > /dev/null 2>&1; then
    echo "Metrics endpoint not available"
    exit 1
fi

# Check if we can reach required services
SERVICES=(
    "mock-governance-1:50051"
    "mock-governance-2:50051"
    "mock-governance-3:50051"
    "mock-bitcoin-node:18332"
    "mock-ethereum-node:8545"
    "prometheus:9090"
)

for service in "${SERVICES[@]}"; do
    if ! timeout 5 bash -c "</dev/tcp/${service//:/ }" 2>/dev/null; then
        echo "Cannot reach service: $service"
        exit 1
    fi
done

# Check memory usage
MEMORY_USAGE=$(ps -o pid,ppid,cmd,%mem --sort=-%mem | grep test-runner | head -1 | awk '{print $4}' | cut -d. -f1)
if [ ! -z "$MEMORY_USAGE" ] && [ "$MEMORY_USAGE" -gt 80 ]; then
    echo "High memory usage: ${MEMORY_USAGE}%"
    exit 1
fi

echo "Health check passed"
exit 0