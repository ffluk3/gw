pub mod human;
pub mod json;
pub mod table;

pub use human::{describe_reason, short_age};
pub use json::to_json;
pub use table::render_table;
