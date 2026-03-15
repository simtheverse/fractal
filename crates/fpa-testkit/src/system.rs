//! System: batch test runner for FPA systems (FPA-034).
//!
//! Wraps the composition function with a one-shot run method for batch
//! testing and reference generation. For interactive or event-driven
//! applications, use `fpa_compositor::compose::compose()` directly and
//! drive the compositor from the application's own event loop.

use std::sync::Arc;

use fpa_bus::Bus;
use fpa_compositor::compose::{compose, ComposeError, PartitionRegistry};
use fpa_compositor::compositor::Compositor;
use fpa_config::CompositionFragment;
use fpa_contract::PartitionError;

/// Error type for system-level operations.
#[derive(Debug)]
pub enum SystemError {
    Partition(PartitionError),
    Config(String),
    Compose(ComposeError),
}

impl std::fmt::Display for SystemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SystemError::Partition(e) => write!(f, "{}", e),
            SystemError::Config(msg) => write!(f, "config error: {}", msg),
            SystemError::Compose(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for SystemError {}

impl From<PartitionError> for SystemError {
    fn from(e: PartitionError) -> Self {
        SystemError::Partition(e)
    }
}

impl From<ComposeError> for SystemError {
    fn from(e: ComposeError) -> Self {
        SystemError::Compose(e)
    }
}

/// A composed FPA system for batch execution.
///
/// For interactive or event-driven applications, use
/// `fpa_compositor::compose::compose()` directly.
pub struct System {
    compositor: Compositor,
    /// Timestep from the fragment's system config, if specified.
    dt: Option<f64>,
}

impl System {
    /// Build a system from a composition fragment.
    pub fn from_fragment(
        fragment: &CompositionFragment,
        registry: &PartitionRegistry,
        bus: Arc<dyn Bus>,
    ) -> Result<Self, SystemError> {
        // Extract timestep from system config if present.
        let dt = fragment
            .system
            .get("timestep")
            .and_then(|v| v.as_float());

        let compositor = compose(fragment, registry, bus)?;
        Ok(System { compositor, dt })
    }

    /// Run the system for a given number of ticks.
    ///
    /// Uses the timestep from the fragment's system config if available,
    /// otherwise uses the provided `dt`.
    ///
    /// Performs: init -> run_tick x N -> dump -> shutdown -> return state.
    /// Shutdown is always attempted after init, even if a later step fails.
    pub fn run(&mut self, ticks: u64, dt: f64) -> Result<toml::Value, SystemError> {
        let actual_dt = self.dt.unwrap_or(dt);
        if let Err(init_err) = self.compositor.init() {
            // Best-effort shutdown for any partitions that initialized
            // before the failure. Log shutdown errors but prioritize the
            // init error since it's the root cause.
            if let Err(shutdown_err) = self.compositor.shutdown() {
                eprintln!(
                    "warning: shutdown after init failure also failed: {shutdown_err}"
                );
            }
            return Err(init_err.into());
        }

        let result = (|| {
            for _ in 0..ticks {
                self.compositor.run_tick(actual_dt)?;
            }
            self.compositor.dump().map_err(SystemError::from)
        })();

        // Always attempt shutdown, even on error.
        let shutdown_result = self.compositor.shutdown().map_err(SystemError::from);

        // Return the first error encountered (tick/dump error takes priority).
        let state = result?;
        shutdown_result?;
        Ok(state)
    }

    /// The timestep from the fragment's system config, if specified.
    pub fn dt(&self) -> Option<f64> {
        self.dt
    }

    /// Access the compositor for advanced operations.
    pub fn compositor(&self) -> &Compositor {
        &self.compositor
    }

    /// Mutably access the compositor for advanced operations.
    pub fn compositor_mut(&mut self) -> &mut Compositor {
        &mut self.compositor
    }
}
