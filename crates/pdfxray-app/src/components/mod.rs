//! Shared widgets of the design (frame "Components") and the small pieces they are made of.
//!
//! Each takes plain data and returns what the user did, so the panels that use them stay free of
//! styling code.

pub mod badge;
pub mod chip;
pub mod context_menu;
pub mod crumb;
pub mod focus;
pub mod icon;
pub mod keycap;
pub mod object_preview;
pub mod popover;
pub mod prop_hints;
pub mod prop_row;
pub mod section;
pub mod segmented;
pub mod step_track;
pub mod switch;
pub mod tool_button;
pub mod tooltip;
pub mod tree_row;

pub use badge::Badge;
pub use chip::{Chip, Tone};
pub use crumb::Crumb;
pub use icon::Icon;
pub use keycap::Keycap;
pub use prop_row::PropRow;
pub use segmented::{
    Segment, segmented, segmented_compact, segmented_mini, segmented_mini_width, segmented_width,
};
pub use step_track::{StepTrack, Tick, TickKind};
pub use switch::switch;
pub use tool_button::ToolButton;
pub use tree_row::{Annotation, TreeRow};
