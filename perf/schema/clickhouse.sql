CREATE DATABASE IF NOT EXISTS solar_perf;

CREATE TABLE IF NOT EXISTS solar_perf.runs (
  workflow_run_id UInt64,
  commit FixedString(40),
  branch Nullable(String),
  pr Nullable(UInt32),
  title Nullable(String),
  started_at DateTime64(3, 'UTC'),
  workflow_name LowCardinality(String),
  source_schema UInt16,
  raw_results String CODEC(ZSTD(6)),
  imported_at DateTime64(3, 'UTC') DEFAULT now64(3)
) ENGINE = ReplacingMergeTree(imported_at)
ORDER BY workflow_run_id;

ALTER TABLE solar_perf.runs ADD COLUMN IF NOT EXISTS title Nullable(String) AFTER pr;

CREATE TABLE IF NOT EXISTS solar_perf.benchmark_results (
  workflow_run_id UInt64,
  commit FixedString(40),
  test_id LowCardinality(String),
  description String,
  suite LowCardinality(String),
  compiler LowCardinality(String),
  status LowCardinality(String),
  compile_time_seconds Nullable(Float64),
  bytecode_size Nullable(UInt64),
  runtime_size Nullable(UInt64),
  deploy_gas Nullable(UInt64),
  total_gas Nullable(UInt64),
  peak_rss_bytes Nullable(UInt64),
  imported_at DateTime64(3, 'UTC') DEFAULT now64(3)
) ENGINE = ReplacingMergeTree(imported_at)
PARTITION BY toYYYYMM(imported_at)
ORDER BY (workflow_run_id, test_id, compiler);

CREATE TABLE IF NOT EXISTS solar_perf.artifact_files (
  workflow_run_id UInt64,
  commit FixedString(40),
  test_id LowCardinality(String),
  compiler LowCardinality(String),
  path LowCardinality(String),
  storage_path LowCardinality(String),
  label String,
  language LowCardinality(String),
  content String CODEC(ZSTD(9)),
  content_sha256 FixedString(64),
  imported_at DateTime64(3, 'UTC') DEFAULT now64(3)
) ENGINE = ReplacingMergeTree(imported_at)
PARTITION BY toYYYYMM(imported_at)
ORDER BY (workflow_run_id, test_id, compiler, storage_path);
