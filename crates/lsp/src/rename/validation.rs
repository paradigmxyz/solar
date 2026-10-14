//! Validates a rename against the compiler's name resolution in each original analysis batch.
//!
//! Declaration locations, mapped through the edits, identify bindings across compiler runs; HIR
//! IDs do not. Comparing every occurrence also catches capture of references that are not edited,
//! while separate batch snapshots preserve import and project configuration. Validation never
//! publishes speculative results. All consulted source snapshots are rechecked before edits are
//! returned, and new compiler errors reject collisions even when neither declaration is used.
//! Bindings are materialized on demand or before aggregation changes their batch-local IDs.

use super::{RenameCandidate, RenameIndex, RenameTarget};
use crate::{
    diagnostics::DiagnosticMap,
    global_state::analyze_rename,
    proto::{self, LspPositionIndex},
    symbols::{DeclarationSymbol, SymbolId},
};
use async_lsp::{ErrorCode, ResponseError};
use crop::Rope;
use lsp_types::{Diagnostic, DiagnosticSeverity, Location, Position, Range, Url};
use solar_config::CompileOpts;
use solar_interface::data_structures::{
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};

#[derive(Clone, Debug)]
pub(crate) struct RenameAnalysis {
    opts: CompileOpts,
    roots: FxHashSet<PathBuf>,
    sources: FxHashMap<Url, Arc<String>>,
    bindings: OnceLock<FxHashMap<Location, Vec<Location>>>,
    errors: Vec<(Url, Diagnostic)>,
}

impl RenameAnalysis {
    pub(crate) fn new(opts: CompileOpts, roots: FxHashSet<PathBuf>, index: &RenameIndex) -> Self {
        Self {
            opts,
            roots,
            sources: index.analyzed_contents.clone(),
            bindings: OnceLock::new(),
            errors: Vec::new(),
        }
    }

    pub(crate) fn record_diagnostics(&mut self, diagnostics: &DiagnosticMap) {
        self.errors = diagnostics
            .iter()
            .flat_map(|(uri, diagnostics)| {
                diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.severity == Some(DiagnosticSeverity::ERROR))
                    .map(|diagnostic| (uri.clone(), diagnostic.clone()))
            })
            .collect();
    }

    pub(crate) fn contains(&self, uri: &Url) -> bool {
        self.sources.contains_key(uri)
    }

    pub(crate) fn validate(
        &self,
        candidate: &mut RenameCandidate,
        new_name: &str,
    ) -> Result<(), ResponseError> {
        let bindings = self.bindings.get().expect("rename bindings are initialized");
        let edits = RenameEdits::new(&candidate.locations, new_name)?;
        let mut sources = self.sources.clone();
        for (uri, source) in &self.sources {
            if let Some(previous) = candidate.analyzed_contents.insert(uri.clone(), source.clone())
                && previous != *source
            {
                return Err(ResponseError::new(
                    ErrorCode::CONTENT_MODIFIED,
                    "document contents changed since analysis",
                ));
            }
            let Some(file_edits) = edits.files.get(uri) else { continue };
            let index = LspPositionIndex::from_rope(Rope::from(source.as_str()));
            let mut changed = source.as_ref().clone();
            for edit in file_edits.iter().rev() {
                let bytes = index.checked_text_range(edit.range).ok_or_else(binding_conflict)?;
                if changed.get(bytes.clone()) != Some(candidate.old_name.as_str()) {
                    return Err(binding_conflict());
                }
                changed.replace_range(bytes, new_name);
            }
            sources.insert(uri.clone(), Arc::new(changed));
        }
        let result = analyze_rename(self.opts.clone(), &self.roots, &sources);
        let after = result.rename_analysis();
        let after_bindings = after.bindings.get().expect("rename bindings are initialized");
        if bindings.len() != after_bindings.len() {
            return Err(binding_conflict());
        }
        for (location, targets) in bindings {
            let mapped = edits.location(location).ok_or_else(binding_conflict)?;
            let targets = targets
                .iter()
                .map(|target| edits.location(target))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(binding_conflict)?;
            if after_bindings.get(&mapped) != Some(&targets) {
                return Err(binding_conflict());
            }
        }
        for (uri, error) in &after.errors {
            if !self.errors.iter().any(|(old_uri, old)| {
                old_uri == uri
                    && edits.range(uri, old.range) == Some(error.range)
                    && old.code == error.code
                    && (old.message == error.message
                        || old.message.replace(&candidate.old_name, new_name) == error.message)
            }) {
                return Err(binding_conflict());
            }
        }
        Ok(())
    }

    pub(crate) fn initialize_bindings(
        &self,
        index: &RenameIndex,
        declarations: &IndexVec<SymbolId, DeclarationSymbol>,
        references: impl Iterator<Item = (Location, Vec<Location>)>,
    ) {
        self.bindings.get_or_init(|| {
            let mut bindings = FxHashMap::<_, Vec<_>>::default();
            for occurrence in &index.occurrences {
                let targets = occurrence.targets.iter().map(|target| match *target {
                    RenameTarget::Symbol(id) => {
                        let declaration = &declarations[id];
                        Location::new(declaration.location.uri.clone(), declaration.name_range)
                    }
                    RenameTarget::ImportAlias(id) => index.aliases[id].location.clone(),
                    RenameTarget::MappingName(id) => index.mapping_names[id].location.clone(),
                });
                bindings.entry(occurrence.location.clone()).or_default().extend(targets);
            }
            // Include references that cannot themselves be renamed, such as ambiguous overloads.
            for (location, targets) in references {
                bindings.entry(location).or_default().extend(targets);
            }
            for targets in bindings.values_mut() {
                targets.sort_unstable_by(|a, b| {
                    (&a.uri, proto::range_key(a.range)).cmp(&(&b.uri, proto::range_key(b.range)))
                });
                targets.dedup();
            }
            bindings
        });
    }

    #[cfg(test)]
    pub(crate) fn bindings_initialized(&self) -> bool {
        self.bindings.get().is_some()
    }
}

struct RenameEdits {
    files: FxHashMap<Url, Vec<RenameEdit>>,
}

struct RenameEdit {
    range: Range,
    shift_after: i64,
}

impl RenameEdits {
    fn new(locations: &[Location], new_name: &str) -> Result<Self, ResponseError> {
        let width =
            i64::try_from(new_name.encode_utf16().count()).map_err(|_| binding_conflict())?;
        let mut files = FxHashMap::<_, Vec<RenameEdit>>::default();
        for location in locations {
            let range = location.range;
            if range.start.line != range.end.line {
                return Err(binding_conflict());
            }
            let edits = files.entry(location.uri.clone()).or_default();
            let previous = edits.last().filter(|edit| edit.range.end.line == range.start.line);
            let shift = previous.map_or(0, |edit| edit.shift_after);
            edits.push(RenameEdit {
                range,
                shift_after: shift + width - i64::from(range.end.character - range.start.character),
            });
        }
        Ok(Self { files })
    }

    fn location(&self, location: &Location) -> Option<Location> {
        Some(Location::new(location.uri.clone(), self.range(&location.uri, location.range)?))
    }

    fn range(&self, uri: &Url, range: Range) -> Option<Range> {
        Some(Range::new(self.position(uri, range.start)?, self.position(uri, range.end)?))
    }

    fn position(&self, uri: &Url, position: Position) -> Option<Position> {
        let Some(edits) = self.files.get(uri) else { return Some(position) };
        let end = edits.partition_point(|edit| edit.range.end <= position);
        if edits.get(end).is_some_and(|edit| edit.range.start < position) {
            return None;
        }
        let shift = end
            .checked_sub(1)
            .map(|index| &edits[index])
            .filter(|edit| edit.range.end.line == position.line)
            .map_or(0, |edit| edit.shift_after);
        let character = u32::try_from(i64::from(position.character) + shift).ok()?;
        Some(Position::new(position.line, character))
    }
}

fn binding_conflict() -> ResponseError {
    ResponseError::new(
        ErrorCode::REQUEST_FAILED,
        "cannot rename this symbol because the new name changes reference bindings or conflicts with another declaration",
    )
}
