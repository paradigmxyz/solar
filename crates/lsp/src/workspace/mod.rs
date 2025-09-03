//! Workspace models.
//!
//! Solar LSP supports multiple workspace models that are configured in different ways.
//!
//! This module contains a generic workspace concept, as well as implementations of different
//! project models (e.g. Foundry projects), and a project discovery algorithm to try and determine
//! what kind of project the LSP is dealing with based on different heuristics.
//!
//! Once a project type is identified, the configuration for that project model is merged into the
//! overall LSP config.
use crate::workspace::foundry::FoundryConfig;

mod foundry;
pub(crate) mod manifest;

#[derive(Debug)]
pub(crate) struct Workspace {
    pub(crate) kind: WorkspaceKind,
}

#[derive(Debug)]
pub(crate) enum WorkspaceKind {
    Foundry {
        foundry: FoundryConfig,
    },
    /// A naked workspace is a workspace with no specific configuration.
    ///
    /// Naked workspaces have no remappings or toolchain-style dependencies, so all imports are
    /// assumed to be relative to the file being parsed.
    Naked,
}

impl Workspace {
    pub(crate) fn load() -> Self {
        todo!()
    }
}
