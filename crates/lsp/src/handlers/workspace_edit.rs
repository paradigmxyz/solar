use crate::{
    code_actions::{CodeActionPlan, ranges_overlap, rope_source_fingerprint},
    document_links::ImportEditPlan,
    proto,
    rename::RenameCandidate,
    vfs::Vfs,
};
use async_lsp::{ErrorCode, ResponseError};
use crop::Rope;
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, DocumentChanges, OneOf,
    OptionalVersionedTextDocumentIdentifier, TextDocumentEdit, TextEdit, Url, WorkspaceEdit,
};
use solar_interface::{
    data_structures::sync::RwLock, diagnostics::Applicability, source_map::SourceMap,
};
use std::{collections::HashMap, sync::Arc};

pub(super) fn validated_rename_workspace_edit(
    candidate: RenameCandidate,
    new_name: String,
    vfs: Arc<RwLock<Vfs>>,
    document_changes: bool,
) -> Result<WorkspaceEdit, ResponseError> {
    Ok(validate_rename(candidate, new_name, vfs)?.into_workspace_edit(document_changes))
}

pub(super) fn validated_import_workspace_edit(
    plan: ImportEditPlan,
    vfs: Arc<RwLock<Vfs>>,
    document_changes: bool,
) -> Result<WorkspaceEdit, ResponseError> {
    Ok(validate_import_edits(plan, vfs)?.into_workspace_edit(document_changes))
}

pub(super) fn validated_code_actions(
    params: CodeActionParams,
    diagnostics: Vec<lsp_types::Diagnostic>,
    vfs: Arc<RwLock<Vfs>>,
    document_changes: bool,
    is_preferred: bool,
    diagnostic_data: bool,
) -> Vec<CodeActionOrCommand> {
    let uri = params.text_document.uri.clone();
    let source_map = SourceMap::empty();
    let Some((contents, version)) = current_file_contents(&vfs, &source_map, &uri) else {
        return Vec::new();
    };
    let plans = crate::code_actions::plans(&params, &diagnostics, &contents);
    if plans.is_empty() {
        return Vec::new();
    }
    let fingerprint = rope_source_fingerprint(&contents);
    plans
        .into_iter()
        .filter_map(|plan| {
            validated_code_action(
                plan,
                &contents,
                version,
                &fingerprint,
                document_changes,
                is_preferred,
                diagnostic_data,
            )
        })
        .collect()
}

fn validated_code_action(
    plan: CodeActionPlan,
    contents: &Rope,
    version: Option<i32>,
    fingerprint: &str,
    document_changes: bool,
    supports_is_preferred: bool,
    supports_diagnostic_data: bool,
) -> Option<CodeActionOrCommand> {
    if plan.source_fingerprint != fingerprint {
        return None;
    }
    let edits = validate_code_action_edits(contents, plan.edits)?;
    let edit = if document_changes {
        WorkspaceEdit {
            changes: None,
            document_changes: Some(DocumentChanges::Edits(vec![TextDocumentEdit {
                text_document: OptionalVersionedTextDocumentIdentifier { uri: plan.uri, version },
                edits: edits.into_iter().map(OneOf::Left).collect(),
            }])),
            change_annotations: None,
        }
    } else {
        WorkspaceEdit {
            changes: Some(HashMap::from([(plan.uri, edits)])),
            document_changes: None,
            change_annotations: None,
        }
    };
    let mut diagnostic = plan.diagnostic;
    if !supports_diagnostic_data {
        diagnostic.data = None;
    }
    Some(
        CodeAction {
            title: plan.title,
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diagnostic]),
            edit: Some(edit),
            is_preferred: supports_is_preferred
                .then_some(plan.applicability == Applicability::MachineApplicable),
            ..Default::default()
        }
        .into(),
    )
}

fn validate_code_action_edits(contents: &Rope, edits: Vec<TextEdit>) -> Option<Vec<TextEdit>> {
    if edits.is_empty() {
        return None;
    }
    let index = proto::LspPositionIndex::new(contents);
    let mut byte_ranges = Vec::with_capacity(edits.len());
    for edit in &edits {
        let range = index.checked_text_range(edit.range)?;
        if index.position_at_byte(range.start) != Some(edit.range.start)
            || index.position_at_byte(range.end) != Some(edit.range.end)
        {
            return None;
        }
        byte_ranges.push(range);
    }
    (!ranges_overlap(&mut byte_ranges)).then_some(edits)
}

struct ValidatedWorkspaceEdit {
    changes: HashMap<Url, Vec<TextEdit>>,
    versions: HashMap<Url, Option<i32>>,
}

impl ValidatedWorkspaceEdit {
    fn into_workspace_edit(mut self, document_changes: bool) -> WorkspaceEdit {
        if !document_changes {
            return WorkspaceEdit {
                changes: Some(self.changes),
                document_changes: None,
                change_annotations: None,
            };
        }

        let edits = self
            .changes
            .into_iter()
            .map(|(uri, edits)| TextDocumentEdit {
                text_document: OptionalVersionedTextDocumentIdentifier {
                    version: self.versions.remove(&uri).unwrap_or(None),
                    uri,
                },
                edits: edits.into_iter().map(OneOf::Left).collect(),
            })
            .collect();
        WorkspaceEdit {
            changes: None,
            document_changes: Some(DocumentChanges::Edits(edits)),
            change_annotations: None,
        }
    }
}

fn validate_rename(
    candidate: RenameCandidate,
    new_name: String,
    vfs: Arc<RwLock<Vfs>>,
) -> Result<ValidatedWorkspaceEdit, ResponseError> {
    if candidate.conflicting_contents {
        return Err(content_modified());
    }
    let mut contents = HashMap::<Url, (Rope, Option<i32>)>::new();
    let source_map = SourceMap::empty();
    for (uri, analyzed_contents) in &candidate.analyzed_contents {
        let Some((file_contents, version)) = current_file_contents(&vfs, &source_map, uri) else {
            return Err(content_modified());
        };
        if file_contents.byte_slice(..) != analyzed_contents.as_str() {
            return Err(content_modified());
        }
        contents.insert(uri.clone(), (file_contents, version));
    }

    for location in &candidate.locations {
        let Some((contents, _)) = contents.get(&location.uri) else {
            return Err(content_modified());
        };
        let Some(range) = proto::checked_text_range(contents, location.range) else {
            return Err(content_modified());
        };
        if contents.byte_slice(range) != candidate.old_name.as_str() {
            return Err(content_modified());
        }
    }

    let mut changes = HashMap::<Url, Vec<TextEdit>>::new();
    for location in candidate.locations {
        changes
            .entry(location.uri)
            .or_default()
            .push(TextEdit::new(location.range, new_name.clone()));
    }
    let versions = contents.into_iter().map(|(uri, (_, version))| (uri, version)).collect();
    Ok(ValidatedWorkspaceEdit { changes, versions })
}

fn validate_import_edits(
    plan: ImportEditPlan,
    vfs: Arc<RwLock<Vfs>>,
) -> Result<ValidatedWorkspaceEdit, ResponseError> {
    let source_map = SourceMap::empty();
    let mut changes = HashMap::new();
    let mut versions = HashMap::new();
    for (uri, planned) in plan.into_entries() {
        let (analyzed_contents, edits) = planned.into_parts();
        let Some((file_contents, version)) = current_file_contents(&vfs, &source_map, &uri) else {
            return Err(content_modified());
        };
        if file_contents.byte_slice(..) != analyzed_contents.as_str() {
            return Err(content_modified());
        }
        versions.insert(uri.clone(), version);
        changes.insert(uri, edits);
    }
    Ok(ValidatedWorkspaceEdit { changes, versions })
}

fn current_file_contents(
    vfs: &RwLock<Vfs>,
    source_map: &SourceMap,
    uri: &Url,
) -> Option<(Rope, Option<i32>)> {
    let path = proto::vfs_path(uri)?;
    let vfs = vfs.read();
    if let Some(contents) = vfs.get_file_contents(&path) {
        return Some((contents.clone(), vfs.get_file_version(&path)));
    }
    drop(vfs);
    let contents = source_map.file_loader().load_file(path.as_path()?).ok()?;
    Some((Rope::from(contents), None))
}

fn content_modified() -> ResponseError {
    ResponseError::new(ErrorCode::CONTENT_MODIFIED, "document contents changed since analysis")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Position, Range};

    #[test]
    fn code_action_edit_validation_accepts_adjacent_utf16_ranges() {
        let contents = Rope::from("😀value");
        let edits = vec![
            TextEdit::new(Range::new(Position::new(0, 0), Position::new(0, 2)), "x".into()),
            TextEdit::new(Range::new(Position::new(0, 2), Position::new(0, 7)), "y".into()),
        ];

        assert_eq!(validate_code_action_edits(&contents, edits.clone()), Some(edits));
    }

    #[test]
    fn code_action_edit_validation_rejects_untrusted_ranges() {
        let contents = Rope::from("😀value");
        let invalid = [
            vec![TextEdit::new(
                Range::new(Position::new(0, 1), Position::new(0, 2)),
                "split surrogate".into(),
            )],
            vec![TextEdit::new(
                Range::new(Position::new(0, 99), Position::new(0, 99)),
                "past line end".into(),
            )],
            vec![
                TextEdit::new(
                    Range::new(Position::new(0, 2), Position::new(0, 5)),
                    "overlap one".into(),
                ),
                TextEdit::new(
                    Range::new(Position::new(0, 4), Position::new(0, 7)),
                    "overlap two".into(),
                ),
            ],
            vec![
                TextEdit::new(Range::new(Position::new(0, 2), Position::new(0, 2)), "first".into()),
                TextEdit::new(
                    Range::new(Position::new(0, 2), Position::new(0, 2)),
                    "second".into(),
                ),
            ],
        ];

        for edits in invalid {
            assert_eq!(validate_code_action_edits(&contents, edits), None);
        }
    }
}
