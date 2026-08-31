//! Core logic for OptiScaler Manager: game detection, artwork resolution and
//! OptiScaler install management. Deliberately free of any UI dependency so it
//! can be unit tested without a display.

pub mod anticheat;
pub mod artwork;
pub mod detect;
pub mod exe_heuristics;
pub mod model;
pub mod optiscaler;
pub mod paths;
pub mod settings;
pub mod text;
pub mod vdf;

pub use detect::detect_all;
pub use model::{Game, GameId, Store};
pub use settings::Settings;
