use std::{cell::Cell, fmt};

pub use fmt::*;

/// Wrapper for [`fmt::from_fn`].
#[cfg(feature = "nightly")]
pub fn from_fn<F: Fn(&mut fmt::Formatter<'_>) -> fmt::Result>(
    f: F,
) -> impl fmt::Debug + fmt::Display {
    fmt::from_fn(f)
}

/// Polyfill for [`fmt::from_fn`].
#[cfg(not(feature = "nightly"))]
pub fn from_fn<F: Fn(&mut fmt::Formatter<'_>) -> fmt::Result>(
    f: F,
) -> impl fmt::Debug + fmt::Display {
    struct FromFn<F>(F);

    impl<F> fmt::Debug for FromFn<F>
    where
        F: Fn(&mut fmt::Formatter<'_>) -> fmt::Result,
    {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            (self.0)(f)
        }
    }

    impl<F> fmt::Display for FromFn<F>
    where
        F: Fn(&mut fmt::Formatter<'_>) -> fmt::Result,
    {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            (self.0)(f)
        }
    }

    FromFn(f)
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
}
