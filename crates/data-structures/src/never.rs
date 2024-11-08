/// Hack to access the never type on stable.
#[cfg(not(feature = "nightly"))]
#[doc(hidden)]
#[allow(unnameable_types)]
pub trait GetReturnType {
    type ReturnType;
}

#[cfg(not(feature = "nightly"))]
#[doc(hidden)]
impl<T> GetReturnType for fn() -> T {
    type ReturnType = T;
}

/// The [`!` (never)](primitive@never) type.
#[cfg(not(feature = "nightly"))]
pub type Never = <fn() -> ! as GetReturnType>::ReturnType;

/// The [`!` (never)](primitive@never) type.
#[cfg(feature = "nightly")]
pub type Never = !;

#[cfg(test)]
mod tests {
    use super::*;

    fn never_returns() -> Never {
        panic!();
    }

    #[test]
    fn test_never_returns() {
        #[cfg(feature = "nightly")]
        fn test1<F: Fn() -> !>(f: F) {
            let _ = f;
        }
        fn test2(f: fn() -> !) {
            let _ = f;
        }

        #[cfg(feature = "nightly")]
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
