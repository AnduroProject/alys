-- Initial schema for Alys V2 Test Coordinator database
-- This schema supports test execution tracking, results storage, and historical analysis

-- Test runs table for tracking test execution
CREATE TABLE test_runs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    test_type TEXT NOT NULL,
    status TEXT NOT NULL,
    start_time DATETIME NOT NULL,
    end_time DATETIME,
    duration_seconds REAL,
    git_commit TEXT,
    git_branch TEXT,
    environment TEXT DEFAULT 'docker',
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Test results table for individual test outcomes
CREATE TABLE test_results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    test_run_id TEXT NOT NULL,
    test_name TEXT NOT NULL,
    test_category TEXT NOT NULL,
    status TEXT NOT NULL, -- passed, failed, skipped
    duration_seconds REAL,
    error_message TEXT,
    stack_trace TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (test_run_id) REFERENCES test_runs(id) ON DELETE CASCADE
);

-- Coverage data table for tracking code coverage over time
CREATE TABLE coverage_data (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    test_run_id TEXT NOT NULL,
    overall_percentage REAL NOT NULL,
    lines_covered INTEGER NOT NULL,
    lines_total INTEGER NOT NULL,
    functions_covered INTEGER NOT NULL,
    functions_total INTEGER NOT NULL,
    branches_covered INTEGER NOT NULL,
    branches_total INTEGER NOT NULL,
    threshold_met BOOLEAN NOT NULL DEFAULT FALSE,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (test_run_id) REFERENCES test_runs(id) ON DELETE CASCADE
);

-- File coverage table for per-file coverage tracking
CREATE TABLE file_coverage (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    coverage_data_id INTEGER NOT NULL,
    file_path TEXT NOT NULL,
    lines_covered INTEGER NOT NULL,
    lines_total INTEGER NOT NULL,
    coverage_percentage REAL NOT NULL,
    uncovered_lines TEXT, -- JSON array of line numbers
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (coverage_data_id) REFERENCES coverage_data(id) ON DELETE CASCADE
);

-- Performance benchmarks table for tracking performance over time
CREATE TABLE benchmarks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    test_run_id TEXT NOT NULL,
    benchmark_name TEXT NOT NULL,
    benchmark_category TEXT NOT NULL, -- actor, sync, system
    value REAL NOT NULL,
    unit TEXT NOT NULL,
    baseline_value REAL,
    change_percentage REAL,
    trend_direction TEXT, -- improving, stable, degrading, unknown
    metadata TEXT, -- JSON for additional benchmark metadata
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (test_run_id) REFERENCES test_runs(id) ON DELETE CASCADE
);

-- Performance regressions table for tracking significant degradations
CREATE TABLE performance_regressions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    test_run_id TEXT NOT NULL,
    benchmark_name TEXT NOT NULL,
    current_value REAL NOT NULL,
    baseline_value REAL NOT NULL,
    degradation_percentage REAL NOT NULL,
    severity TEXT NOT NULL, -- critical, major, minor, negligible
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (test_run_id) REFERENCES test_runs(id) ON DELETE CASCADE
);

-- Chaos test results table for chaos engineering experiments
CREATE TABLE chaos_tests (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    test_run_id TEXT NOT NULL,
    experiment_name TEXT NOT NULL,
    fault_type TEXT NOT NULL,
    success BOOLEAN NOT NULL,
    recovery_time_ms INTEGER,
    failure_time_ms INTEGER,
    auto_recovery BOOLEAN DEFAULT FALSE,
    severity TEXT, -- critical, major, minor
    performance_impact TEXT, -- JSON object with impact metrics
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (test_run_id) REFERENCES test_runs(id) ON DELETE CASCADE
);

-- System stability metrics table
CREATE TABLE system_stability (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    test_run_id TEXT NOT NULL,
    mean_time_to_failure REAL,
    mean_time_to_recovery REAL,
    availability_percentage REAL,
    error_rate REAL,
    throughput_degradation REAL,
    resilience_score REAL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (test_run_id) REFERENCES test_runs(id) ON DELETE CASCADE
);

-- Service health tracking table
CREATE TABLE service_health (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    service_name TEXT NOT NULL,
    status TEXT NOT NULL, -- healthy, degraded, unhealthy, unknown
    response_time_ms INTEGER,
    version TEXT,
    error_message TEXT,
    checked_at DATETIME NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Test artifacts table for tracking generated files and reports
CREATE TABLE test_artifacts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    test_run_id TEXT NOT NULL,
    artifact_type TEXT NOT NULL, -- coverage_report, benchmark_report, flamegraph, etc.
    file_path TEXT NOT NULL,
    file_size INTEGER,
    mime_type TEXT,
    description TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (test_run_id) REFERENCES test_runs(id) ON DELETE CASCADE
);

-- Indexes for better query performance
CREATE INDEX idx_test_runs_start_time ON test_runs(start_time);
CREATE INDEX idx_test_runs_status ON test_runs(status);
CREATE INDEX idx_test_runs_git_commit ON test_runs(git_commit);
CREATE INDEX idx_test_runs_test_type ON test_runs(test_type);

CREATE INDEX idx_test_results_run_id ON test_results(test_run_id);
CREATE INDEX idx_test_results_status ON test_results(status);
CREATE INDEX idx_test_results_category ON test_results(test_category);

CREATE INDEX idx_coverage_data_run_id ON coverage_data(test_run_id);
CREATE INDEX idx_coverage_data_percentage ON coverage_data(overall_percentage);

CREATE INDEX idx_file_coverage_data_id ON file_coverage(coverage_data_id);
CREATE INDEX idx_file_coverage_path ON file_coverage(file_path);

CREATE INDEX idx_benchmarks_run_id ON benchmarks(test_run_id);
CREATE INDEX idx_benchmarks_name ON benchmarks(benchmark_name);
CREATE INDEX idx_benchmarks_category ON benchmarks(benchmark_category);
CREATE INDEX idx_benchmarks_created_at ON benchmarks(created_at);

CREATE INDEX idx_performance_regressions_run_id ON performance_regressions(test_run_id);
CREATE INDEX idx_performance_regressions_severity ON performance_regressions(severity);

CREATE INDEX idx_chaos_tests_run_id ON chaos_tests(test_run_id);
CREATE INDEX idx_chaos_tests_fault_type ON chaos_tests(fault_type);
CREATE INDEX idx_chaos_tests_success ON chaos_tests(success);

CREATE INDEX idx_system_stability_run_id ON system_stability(test_run_id);

CREATE INDEX idx_service_health_service_name ON service_health(service_name);
CREATE INDEX idx_service_health_checked_at ON service_health(checked_at);

CREATE INDEX idx_test_artifacts_run_id ON test_artifacts(test_run_id);
CREATE INDEX idx_test_artifacts_type ON test_artifacts(artifact_type);

-- Views for common queries

-- Latest test run summary view
CREATE VIEW latest_test_run_summary AS
SELECT 
    tr.id,
    tr.name,
    tr.test_type,
    tr.status,
    tr.start_time,
    tr.end_time,
    tr.duration_seconds,
    tr.git_commit,
    tr.git_branch,
    COUNT(DISTINCT tres.id) as total_tests,
    SUM(CASE WHEN tres.status = 'passed' THEN 1 ELSE 0 END) as passed_tests,
    SUM(CASE WHEN tres.status = 'failed' THEN 1 ELSE 0 END) as failed_tests,
    SUM(CASE WHEN tres.status = 'skipped' THEN 1 ELSE 0 END) as skipped_tests,
    ROUND(
        (SUM(CASE WHEN tres.status = 'passed' THEN 1 ELSE 0 END) * 100.0 / 
         NULLIF(COUNT(DISTINCT tres.id), 0)), 2
    ) as success_rate,
    cd.overall_percentage as coverage_percentage
FROM test_runs tr
LEFT JOIN test_results tres ON tr.id = tres.test_run_id
LEFT JOIN coverage_data cd ON tr.id = cd.test_run_id
GROUP BY tr.id, tr.name, tr.test_type, tr.status, tr.start_time, tr.end_time, 
         tr.duration_seconds, tr.git_commit, tr.git_branch, cd.overall_percentage
ORDER BY tr.start_time DESC;

-- Coverage trends view
CREATE VIEW coverage_trends AS
SELECT 
    tr.git_commit,
    tr.start_time,
    cd.overall_percentage,
    cd.threshold_met,
    LAG(cd.overall_percentage) OVER (ORDER BY tr.start_time) as previous_percentage,
    cd.overall_percentage - LAG(cd.overall_percentage) OVER (ORDER BY tr.start_time) as percentage_change
FROM test_runs tr
JOIN coverage_data cd ON tr.id = cd.test_run_id
WHERE tr.status = 'completed'
ORDER BY tr.start_time DESC;

-- Performance trends view
CREATE VIEW performance_trends AS
SELECT 
    b.benchmark_name,
    b.benchmark_category,
    tr.start_time,
    tr.git_commit,
    b.value,
    b.unit,
    LAG(b.value) OVER (PARTITION BY b.benchmark_name ORDER BY tr.start_time) as previous_value,
    b.value - LAG(b.value) OVER (PARTITION BY b.benchmark_name ORDER BY tr.start_time) as value_change,
    CASE 
        WHEN LAG(b.value) OVER (PARTITION BY b.benchmark_name ORDER BY tr.start_time) IS NOT NULL THEN
            ROUND(((b.value - LAG(b.value) OVER (PARTITION BY b.benchmark_name ORDER BY tr.start_time)) / 
                   LAG(b.value) OVER (PARTITION BY b.benchmark_name ORDER BY tr.start_time)) * 100, 2)
        ELSE NULL
    END as percentage_change
FROM benchmarks b
JOIN test_runs tr ON b.test_run_id = tr.id
WHERE tr.status = 'completed'
ORDER BY b.benchmark_name, tr.start_time DESC;

-- Service health summary view
CREATE VIEW service_health_summary AS
SELECT 
    service_name,
    status,
    AVG(response_time_ms) as avg_response_time_ms,
    COUNT(*) as check_count,
    MAX(checked_at) as last_check,
    SUM(CASE WHEN status = 'healthy' THEN 1 ELSE 0 END) * 100.0 / COUNT(*) as health_percentage
FROM service_health 
WHERE checked_at >= datetime('now', '-24 hours')
GROUP BY service_name, status
ORDER BY service_name;