/// Splits a string slice at the first whitespace character.
///
/// Returns the content up to the whitespace and the position of the first following non-blank char.
#[inline]
pub fn split_once_ws(content: &str, start: usize, end: usize) -> (&str, usize) {
    let bytes = content.as_bytes();
    if let Some(ws_pos) =
        bytes[start..end].iter().position(|b| b.is_ascii_whitespace()).map(|offset| start + offset)
    {
        let rest = &bytes[ws_pos..end];
        (&content[start..ws_pos], ws_pos + (rest.len() - rest.trim_ascii_start().len()))
    } else {
        (&content[start..end], end)
    }
}

/// Splits a string slice at the end of its leading identifier.
///
/// Returns the identifier prefix and the position of the first following non-blank char; the
/// prefix is empty when the content does not start with an identifier character. This mirrors
/// solc, which parses documented parameter names as identifier prefixes, so `@param -` documents
/// an unnamed parameter.
#[inline]
pub fn split_once_ident(content: &str, start: usize, end: usize) -> (&str, usize) {
    let bytes = content.as_bytes();
    let ident_len = bytes[start..end]
        .iter()
        .position(|&b| !crate::lexer::is_id_continue_byte(b))
        .unwrap_or(end - start);
    let ident_end = start + ident_len;
    let rest = &bytes[ident_end..end];
    (&content[start..ident_end], ident_end + (rest.len() - rest.trim_ascii_start().len()))
}

/// Returns the first non-blank word and the position of the first following non-blank char.
#[inline]
pub fn first_word(content: &str, start: usize, end: usize) -> Option<(&str, usize)> {
    let bytes = &content.as_bytes()[start..end];
    let start = start + (bytes.len() - bytes.trim_ascii_start().len());
    let (word, content_start) = split_once_ws(content, start, end);
    if word.is_empty() { None } else { Some((word, content_start)) }
}
