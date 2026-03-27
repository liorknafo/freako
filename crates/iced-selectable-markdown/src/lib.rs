//! Selectable markdown widget for iced.
//!
//! Renders markdown with native text selection support (click, drag, double-click,
//! triple-click, Ctrl+C to copy).

pub mod state;
pub mod selectable_rich;
pub mod view;

pub use state::{SelectionAction, SelectionState};
pub use view::view;
