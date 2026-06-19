/// Splits a string slice at the first whitespace character.
///
/// Returns the content up to the whitespace and the position of the first following non-blank char.
#[inline]
pub fn split_once_ws(content: &str, start: usize, end: usize) -> (&str, usize) {
    let bytes = content.as_bytes();
    if let Some(ws_pos) =
        memchr::memchr3(b' ', b'\t', b'\r', &bytes[start..end]).map(|offset| start + offset)
    {
        let rest = &bytes[ws_pos..end];
        (&content[start..ws_pos], ws_pos + (rest.len() - rest.trim_ascii_start().len()))
    } else {
        (&content[start..end], end)
    }
}

/// Returns the first non-blank word and the position of the first following non-blank char.
#[inline]
pub fn first_word(content: &str, start: usize, end: usize) -> Option<(&str, usize)> {
    let bytes = &content.as_bytes()[start..end];
    let start = start + (bytes.len() - bytes.trim_ascii_start().len());
    let (word, content_start) = split_once_ws(content, start, end);
    if word.is_empty() { None } else { Some((word, content_start)) }
}
