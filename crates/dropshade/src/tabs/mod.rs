//! Tabs: the model, the panes inside a tab and the tab bar.

pub mod bar;
pub mod model;
pub mod panes;

pub use bar::{BarPosition, BarState, TabMessage, TabSizing};
pub use model::{Attention, TabId, Tabs};
pub use panes::{Panes, SplitDirection};
