//! Rule engine evaluation benchmark.
//!
//! Measures how many rules per second the engine can evaluate
//! against a batch of telemetry data points.
//!
//! To run:
//!   cargo bench --bench rule_evaluation
//!
//! NOTE: This requires a benchmark harness crate to be wired up.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_simple_numeric_rule(c: &mut Criterion) {
    // TODO: wire up to tinyiothub_engine::rule::evaluator::RuleEvaluator
    c.bench_function("evaluate_temperature_threshold", |b| {
        let data = serde_json::json!({ "temperature": 85.0 });
        b.iter(|| black_box(&data));
    });
}

fn bench_complex_nested_rule(c: &mut Criterion) {
    c.bench_function("evaluate_5_condition_and_rule", |b| {
        let data = serde_json::json!({
            "temperature": 85.0,
            "humidity": 70.0,
            "pressure": 1013.0,
            "voltage": 11.5,
            "current": 2.5
        });
        b.iter(|| black_box(&data));
    });
}

criterion_group!(rule_benches, bench_simple_numeric_rule, bench_complex_nested_rule);
criterion_main!(rule_benches);
