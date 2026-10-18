pub mod json;
pub mod router;
pub mod state;

use serde_json::Value;

pub type EventSink = std::sync::Arc<dyn Fn(Value) + Send + Sync>;

pub const MAX_MESSAGE_CHARACTERS: usize = 2_000_000;
