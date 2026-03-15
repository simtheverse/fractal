//! Re-export of PartitionRegistry from fpa-compositor.
//!
//! PartitionRegistry lives in fpa-compositor::compose where it belongs
//! alongside the composition function. This module re-exports it for
//! backward compatibility.

pub use fpa_compositor::compose::PartitionRegistry;
