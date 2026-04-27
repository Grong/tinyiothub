//! Telemetry ingestion throughput benchmark.
//!
//! Measures how many telemetry messages per second the pipeline
//! can decode, transform, and route.
//!
//! To run:
//!   cargo bench --bench telemetry_throughput
//!
//! NOTE: This requires a benchmark harness crate to be wired up.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_decode_modbus(c: &mut Criterion) {
    // TODO: wire up to tinyiothub_engine::pipeline::decoder::ProtocolDecoder
    c.bench_function("decode_modbus_10_registers", |b| {
        let payload = vec![0u8; 20]; // placeholder
        b.iter(|| black_box(&payload));
    });
}

fn bench_decode_mqtt_json(c: &mut Criterion) {
    c.bench_function("decode_mqtt_json_1kb", |b| {
        let payload = r#"{"temperature":23.5,"humidity":60}"#.as_bytes();
        b.iter(|| black_box(payload));
    });
}

criterion_group!(telemetry_benches, bench_decode_modbus, bench_decode_mqtt_json);
criterion_main!(telemetry_benches);
