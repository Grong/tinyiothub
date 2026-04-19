//! Metric registry for collecting and querying metrics.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::meter::{Counter, Gauge, Histogram, MetricValue};

/// Errors that can occur during registry operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    DuplicateMetric(String),
    MetricNotFound(String),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::DuplicateMetric(name) => {
                write!(f, "metric '{}' already registered", name)
            }
            RegistryError::MetricNotFound(name) => {
                write!(f, "metric '{}' not found", name)
            }
        }
    }
}

impl std::error::Error for RegistryError {}

/// A registry that holds all metrics for a component.
#[derive(Debug, Default)]
pub struct MetricRegistry {
    counters: Mutex<HashMap<String, Arc<Counter>>>,
    gauges: Mutex<HashMap<String, Arc<Gauge>>>,
    histograms: Mutex<HashMap<String, Arc<Histogram>>>,
}

impl MetricRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_counter(
        &self,
        name: impl Into<String>,
    ) -> Result<Arc<Counter>, RegistryError> {
        let name = name.into();
        let mut counters = self.counters.lock().unwrap();
        if counters.contains_key(&name) {
            return Err(RegistryError::DuplicateMetric(name));
        }
        let counter = Arc::new(Counter::new(name.clone()));
        counters.insert(name, counter.clone());
        Ok(counter)
    }

    pub fn register_gauge(&self, name: impl Into<String>) -> Result<Arc<Gauge>, RegistryError> {
        let name = name.into();
        let mut gauges = self.gauges.lock().unwrap();
        if gauges.contains_key(&name) {
            return Err(RegistryError::DuplicateMetric(name));
        }
        let gauge = Arc::new(Gauge::new(name.clone()));
        gauges.insert(name, gauge.clone());
        Ok(gauge)
    }

    pub fn register_histogram(
        &self,
        name: impl Into<String>,
        buckets: Vec<f64>,
    ) -> Result<Arc<Histogram>, RegistryError> {
        let name = name.into();
        let mut histograms = self.histograms.lock().unwrap();
        if histograms.contains_key(&name) {
            return Err(RegistryError::DuplicateMetric(name));
        }
        let histogram = Arc::new(Histogram::new(name.clone(), buckets));
        histograms.insert(name, histogram.clone());
        Ok(histogram)
    }

    pub fn get_counter(&self, name: &str) -> Option<Arc<Counter>> {
        self.counters.lock().unwrap().get(name).cloned()
    }

    pub fn get_gauge(&self, name: &str) -> Option<Arc<Gauge>> {
        self.gauges.lock().unwrap().get(name).cloned()
    }

    pub fn get_histogram(&self, name: &str) -> Option<Arc<Histogram>> {
        self.histograms.lock().unwrap().get(name).cloned()
    }

    /// Snapshot all registered metrics as a flat map.
    pub fn snapshot(&self) -> HashMap<String, MetricValue> {
        let mut result = HashMap::new();

        for (name, counter) in self.counters.lock().unwrap().iter() {
            result.insert(name.clone(), MetricValue::Counter(counter.value()));
        }

        for (name, gauge) in self.gauges.lock().unwrap().iter() {
            result.insert(name.clone(), MetricValue::Gauge(gauge.value()));
        }

        for (name, histogram) in self.histograms.lock().unwrap().iter() {
            let counts = histogram.counts();
            let count = counts.iter().sum();
            result.insert(
                name.clone(),
                MetricValue::Histogram {
                    sum: 0.0, // Histogram does not track sum yet
                    count,
                },
            );
        }

        result
    }
}
