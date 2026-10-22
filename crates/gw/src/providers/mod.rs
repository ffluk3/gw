//! Capability adapters for external systems.
//!
//! A provider answers questions about tickets or changes. The traits here name
//! the capability, not the vendor, so the command layer never learns whether a
//! ticket came from JIRA, Linear, or a local command. Selection lives in
//! [`registry`], and the meaning of a branch lives in `gw_core::convention`.

pub mod command;
pub mod github;
pub mod jira;
pub mod registry;

pub use registry::Registry;

use gw_core::{ChangeObservation, ChangeQuery, ProviderId, TicketId, TicketObservation};

use crate::error::Error;

/// The most queries one provider call may carry.
///
/// Vendors cap request size differently; each provider narrows this further if
/// it must. Keeping the shape in one place stops a magic number from spreading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_queries: usize,
}

impl Default for Limits {
    fn default() -> Self {
        // The original JIRA path requested at most 100 tickets, and the GitHub
        // path chunked at 50. 50 is the safe intersection.
        Self { max_queries: 50 }
    }
}

/// A source of ticket state.
pub trait TicketProvider {
    fn id(&self) -> &ProviderId;

    /// Look up each identifier and return one observation per identifier.
    ///
    /// A provider that cannot answer for one identifier returns an observation
    /// with an `Unknown` status rather than omitting it, so the caller can tell
    /// "asked and got nothing trustworthy" from "never asked".
    fn lookup(&self, ids: &[TicketId], limits: Limits) -> Result<Vec<TicketObservation>, Error>;
}

/// A source of change (pull request, merge request) state.
pub trait ChangeProvider {
    fn id(&self) -> &ProviderId;

    /// Answer each query, one observation per query.
    ///
    /// The provider receives the repository as git reports it plus the
    /// `owner/name` slug when the remote has one. One adapter must not have to
    /// understand another host's remote syntax.
    fn lookup(
        &self,
        repo: &crate::commands::snapshot::RepoRef,
        queries: &[ChangeQuery],
        limits: Limits,
    ) -> Result<Vec<ChangeObservation>, Error>;
}

/// A provider that is configured but cannot run, with the reason.
///
/// Surfacing the reason is the difference between "no PR found" and "gh is not
/// installed": both leave a worktree alone, but only one is worth fixing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unavailable {
    pub id: ProviderId,
    pub reason: String,
}
