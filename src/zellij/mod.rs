pub mod actions;

pub use actions::{TabActionResult, ZellijActions, ZellijMode};

// 後方互換: multiplexer モジュールの型も re-export
pub use crate::multiplexer::{WindowActionResult, MultiplexerBackend};
