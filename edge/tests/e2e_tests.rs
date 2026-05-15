// E2E integration tests that require a mosquitto MQTT broker container.
// All tests are #[ignore] — run manually with:
//
//   docker compose -f edge/tests/e2e/docker-compose.yml up -d
//   cargo build -p tinyiothub-edge
//   cargo test -p tinyiothub-edge --test e2e_tests -- --ignored --test-threads=1
//
mod e2e;
