//! Routes the `log::debug!` events of generated ISLE code to `tracing`.
//!
//! `build.rs` makes every generated rule emit `log::debug!("ISLE {} {}", term, "<file> line <n>")`
//! when it returns. Generated modules import this module as `log`, so each event becomes one
//! `tracing` event under the `solar::isle` target. All rules share the one callsite in [`event`],
//! which keeps the binary small and costs one disabled-callsite check when no subscriber enables
//! the target. The line is 0-based: the rule form is on line `n + 1`.

macro_rules! debug {
    ($format:literal, $term:expr, $location:expr) => {
        $crate::isle_log::event($term, $location)
    };
}

pub(crate) use debug;

#[inline(never)]
pub(crate) fn event(term: &str, location: &str) {
    tracing::debug!(target: "solar::isle", "ISLE {term} {location}");
}
