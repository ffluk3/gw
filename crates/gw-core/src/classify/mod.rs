pub mod add;
pub mod clean;
pub mod verdict;

pub use add::{add_strategy, AddStrategy};
pub use clean::classify_clean;
pub use verdict::{decide, verdict, Verdict};
