//! Pure domain for `gw`.
//!
//! This crate performs no I/O: no filesystem, no processes, no environment, no
//! network. Everything here is a total function over already-collected facts,
//! so the policy that decides what gets destroyed is testable and reviewable in
//! isolation from the adapters that gather those facts.

pub mod classify;
pub mod decision;
pub mod layout;
pub mod model;
pub mod policy;
pub mod render;
pub mod snapshot;

pub use decision::{Counts, Decision, Plan, Reason, Target};
pub use layout::Layout;
pub use model::{BranchName, ModelError, Sha, TicketKey};
pub use policy::Policy;
pub use snapshot::{
    Activity, ActivitySource, BranchSnapshot, Dirty, ReachableSet, RepoSnapshot, TicketStatus,
    WorktreeKind, WorktreeSnapshot,
};
