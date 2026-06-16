use std::{cell::Cell, fmt};

pub use fmt::*;

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
