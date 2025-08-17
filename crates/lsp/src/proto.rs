use crop::Rope;

use crate::vfs::{self, VfsPath};

pub(crate) fn vfs_path(url: &lsp_types::Url) -> Option<vfs::VfsPath> {
    url.to_file_path().map(VfsPath::from).ok()
}

/// Converts an [`lsp_types::Range`] to a [`Range`].
///
/// This assumes the position encoding in LSP is UTF-16, which is mandatory to support in the LSP
/// spec.
///
/// [`Range`]: std::ops::Range
pub(crate) fn text_range(rope: &Rope, range: lsp_types::Range) -> std::ops::Range<usize> {
    let start_line = if range.start.line > rope.line_len() as u32 {
        0usize
    } else {
        rope.byte_of_line(range.start.line as usize)
    };
    let start = rope.byte_of_utf16_code_unit(start_line + range.start.character as usize);
    let end_line = if range.end.line > rope.line_len() as u32 {
        0usize
    } else {
        rope.byte_of_line(range.end.line as usize)
    };
    let end = rope.byte_of_utf16_code_unit(end_line + range.end.character as usize);

    start..end
}
