/// Safe spawn wrapper with panic handling
///
/// Provides panic-safe task spawning that prevents crashes from propagating.
/// All panics are caught and logged instead of crashing the application.
use std::future::Future;
use std::panic;
use tracing::{error, warn};

/// Spawn a task with panic protection
///
/// All panics are caught and logged, preventing application crashes.
/// On HarmonyOS with current_thread runtime, tasks are skipped with a warning.
#[cfg(feature = "harmonyos")]
pub fn spawn_safe<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    // On HarmonyOS with current_thread runtime, we can't spawn
    // Log a warning and skip the task
    warn!("spawn_safe: skipping task on HarmonyOS (current_thread runtime limitation)");
    drop(future);
}

/// Spawn a task with panic protection
///
/// All panics are caught and logged, preventing application crashes.
#[cfg(not(feature = "harmonyos"))]
pub fn spawn_safe<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            // Execute the future in a panic-safe context
            tokio::runtime::Handle::current().block_on(async {
                // Wrap the future execution
                let panic_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                    tokio::runtime::Handle::current().block_on(future)
                }));

                if let Err(e) = panic_result {
                    error!("Task panicked during execution: {:?}", e);
                }
            })
        }));

        if let Err(e) = result {
            error!("Task panicked in outer handler: {:?}", e);
        }
    });
}

/// Spawn a task with error handling and custom name
///
/// On HarmonyOS: logs and skips
/// On other platforms: spawns with panic protection
pub fn spawn_with_error_handling<F, Fut>(name: &'static str, f: F)
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    #[cfg(feature = "harmonyos")]
    {
        warn!(
            "spawn_with_error_handling: skipping '{}' on HarmonyOS",
            name
        );
    }

    #[cfg(not(feature = "harmonyos"))]
    {
        tokio::spawn(async move {
            let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                tokio::runtime::Handle::current().block_on(f())
            }));

            if let Err(e) = result {
                error!("Task '{}' panicked: {:?}", name, e);
            }
        });
    }
}

/// Execute a future inline or spawn it based on platform
///
/// Returns immediately on all platforms
#[cfg(feature = "harmonyos")]
pub async fn execute_or_spawn<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    // On HarmonyOS, skip the task
    warn!("execute_or_spawn: skipping task on HarmonyOS");
    drop(future);
}

/// Execute a future inline or spawn it based on platform
///
/// Returns immediately on all platforms
#[cfg(not(feature = "harmonyos"))]
pub async fn execute_or_spawn<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            tokio::runtime::Handle::current().block_on(future)
        }));

        if let Err(e) = result {
            error!("Spawned task panicked: {:?}", e);
        }
    });
}
