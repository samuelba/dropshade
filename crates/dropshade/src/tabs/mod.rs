//! Tabs: the model and the tab bar.

pub mod bar;
pub mod model;

pub use bar::{BarPosition, BarState, TabMessage, TabSizing};
pub use model::{TabId, Tabs};
