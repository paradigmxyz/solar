use std::{
    cell::{Cell, RefCell},
    fmt,
};

pub use fmt::*;

/// Creates a formatter from a function.
pub fn from_fn<F>(f: F) -> FromFn<F>
where
    F: Fn(&mut fmt::Formatter<'_>) -> fmt::Result,
{
    FromFn(f)
}

/// Display adapter returned by [`from_fn`].
pub struct FromFn<F>(F);

impl<F> fmt::Display for FromFn<F>
where
    F: Fn(&mut fmt::Formatter<'_>) -> fmt::Result,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.0)(f)
    }
}

/// Iterator formatting helpers.
pub trait FmtIteratorExt: Iterator + Sized {
    /// Formats each item separated by `separator`.
    fn format<'a>(self, separator: &'a str) -> Format<'a, Self>
    where
        Self::Item: fmt::Display,
    {
        Format { iter: Cell::new(Some(self)), separator }
    }

    /// Formats each item separated by `separator`, using `format` for each item.
    fn format_with<'a, F>(self, separator: &'a str, format: F) -> FormatWith<'a, Self, F>
    where
        F: FnMut(&mut fmt::Formatter<'_>, Self::Item) -> fmt::Result,
    {
        FormatWith { inner: RefCell::new(Some((self, format))), separator }
    }
}

impl<I: Iterator> FmtIteratorExt for I {}

/// Display adapter returned by [`FmtIteratorExt::format`].
pub struct Format<'a, I> {
    iter: Cell<Option<I>>,
    separator: &'a str,
}

impl<I> fmt::Display for Format<'_, I>
where
    I: Iterator,
    I::Item: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let iter = self.iter.take().expect("format called twice");
        for (i, item) in iter.enumerate() {
            if i != 0 {
                f.write_str(self.separator)?;
            }
            write!(f, "{item}")?;
        }
        Ok(())
    }
}

/// Display adapter returned by [`FmtIteratorExt::format_with`].
pub struct FormatWith<'a, I, F> {
    inner: RefCell<Option<(I, F)>>,
    separator: &'a str,
}

impl<I, F> fmt::Display for FormatWith<'_, I, F>
where
    I: Iterator,
    F: FnMut(&mut fmt::Formatter<'_>, I::Item) -> fmt::Result,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (iter, mut format) = self.inner.borrow_mut().take().expect("format_with called twice");
        for (i, item) in iter.enumerate() {
            if i != 0 {
                f.write_str(self.separator)?;
            }
            format(f, item)?;
        }
        Ok(())
    }
}

/// Returns `list` formatted as a comma-separated list with "or" before the last item.
pub fn or_list<I>(list: I) -> impl fmt::Display
where
    I: IntoIterator<IntoIter: ExactSizeIterator, Item: fmt::Display>,
{
    let list = Cell::new(Some(list.into_iter()));
    from_fn(move |f| {
        let list = list.take().expect("or_list called twice");
        let len = list.len();
        for (i, t) in list.enumerate() {
            if i > 0 {
                let is_last = i == len - 1;
                f.write_str(if len > 2 && is_last {
                    ", or "
                } else if len == 2 && is_last {
                    " or "
                } else {
                    ", "
                })?;
            }
            write!(f, "{t}")?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_or_list() {
        let tests: &[(&[&str], &str)] = &[
            (&[], ""),
            (&["`<eof>`"], "`<eof>`"),
            (&["integer", "identifier"], "integer or identifier"),
            (&["path", "string literal", "`&&`"], "path, string literal, or `&&`"),
            (&["`&&`", "`||`", "`&&`", "`||`"], "`&&`, `||`, `&&`, or `||`"),
        ];
        for &(tokens, expected) in tests {
            assert_eq!(or_list(tokens).to_string(), expected, "{tokens:?}");
        }
    }

    #[test]
    fn test_format() {
        assert_eq!([1, 2, 3].iter().format(", ").to_string(), "1, 2, 3");
    }

    #[test]
    fn test_format_with() {
        let values = [1, 2, 3];
        let formatted = values.iter().format_with(" | ", |f, value| write!(f, "#{value}"));
        assert_eq!(formatted.to_string(), "#1 | #2 | #3");
    }
}
