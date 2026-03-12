//! Event system: condition evaluation, triggers, actions, and snapshot semantics.

pub mod action;
pub mod engine;
pub mod event;
pub mod scope;

pub use action::ActionHandler;
pub use engine::EventEngine;
pub use event::{EventAction, EventDefinition, EventTrigger, Predicate};
pub use scope::ActionRegistry;
