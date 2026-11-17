/// The [`!` (never)](primitive@never) type.
#[doc(hidden)]
pub type Never = !;

#[cfg(test)]
mod tests {
    use super::*;

    fn never_returns() -> Never {
        panic!();
    }

    #[test]
    fn test_never_returns() {
        fn test1<F: Fn() -> !>(f: F) {
            let _ = f;
        }
        fn test2(f: fn() -> !) {
            let _ = f;
        }

        test1(never_returns);
        test2(never_returns);
    }

    #[test]
    fn never() {
        let r = Ok::<i32, Never>(42);

        // This would be an error if `Never` was not exactly the primitive `!` type.
        #[allow(clippy::infallible_destructuring_match)]
        let x = match r {
            Ok(x) => x,
        };
        assert_eq!(x, 42);

        let Ok(x) = r;
        assert_eq!(x, 42);

        let x = if true { 43 } else { never_returns() };
        assert_eq!(x, 43);
    }
}
