//! Pure domain for `gw`.
//!
//! This crate performs no I/O: no filesystem, no processes, no environment, no
//! network. Everything here is a total function over already-collected facts,
//! so the policy that decides what gets destroyed is testable and reviewable in
//! isolation from the adapters that gather those facts.

pub mod classify;
pub mod convention;
pub mod decision;
pub mod duration;
pub mod layout;
pub mod model;
pub mod policy;
pub mod render;
pub mod snapshot;

pub use convention::{
    ChangeQuery, ChangeRule, ChangeSelector, Convention, ProviderId, Resolution, Strength,
    TicketRef, TicketRule, TicketSelector,
};
pub use decision::{Counts, Decision, Plan, Reason, Target};
pub use duration::{format_duration_secs, parse_duration_secs};
pub use layout::Layout;
pub use model::{BranchName, ModelError, Sha, TicketId};
pub use policy::Policy;
pub use snapshot::{
    Activity, ActivitySource, BranchSnapshot, ChangeObservation, ChangeState, Dirty, ReachableSet,
    RepoSnapshot, TicketObservation, TicketStatus, WorktreeKind, WorktreeSnapshot,
};
