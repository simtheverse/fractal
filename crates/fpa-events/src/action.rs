//! Action handler trait for executing triggered event actions.

use crate::event::EventAction;

/// Trait for handling triggered event actions.
///
/// Implementations receive fired actions from the compositor after event
/// evaluation and perform the corresponding side effects (e.g., adjust
/// simulation parameters, emit bus messages, stop the run).
pub trait ActionHandler: Send {
    /// Handle a triggered action. Called by the compositor after event evaluation.
    fn handle_action(&mut self, action: &EventAction) -> Result<(), String>;
}
