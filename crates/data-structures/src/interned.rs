// Modified from [`rustc_data_structures`](https://github.com/rust-lang/rust/blob/f82eb4d0a01e2dc782e582f7081439e172b858f9/compiler/rustc_data_structures/src/intern.rs).

use std::{
    cmp::Ordering,
    fmt::{self, Debug},
    hash::{Hash, Hasher},
    ops::Deref,
    ptr,
};

#[allow(unnameable_types)]
mod private {
    #[derive(Clone, Copy, Debug)]
    pub struct PrivateZst;
}

/// A reference to a value that is interned, and is known to be unique.
///
/// Note that it is possible to have a `T` and a `Interned<T>` that are (or
/// refer to) equal but different values. But if you have two different
/// `Interned<T>`s, they both refer to the same value, at a single location in
/// memory. This means that equality and hashing can be done on the value's
/// address rather than the value's contents, which can improve performance.
///
/// The `PrivateZst` field means you can pattern match with `Interned(v, _)`
/// but you can only construct a `Interned` with `new_unchecked`, and not
/// directly.
#[cfg_attr(feature = "nightly", rustc_pass_by_value)]
pub struct Interned<'a, T>(pub &'a T, pub private::PrivateZst);

impl<'a, T> Interned<'a, T> {
    /// Create a new `Interned` value. The value referred to *must* be interned
    /// and thus be unique, and it *must* remain unique in the future. This
    /// function has `_unchecked` in the name but is not `unsafe`, because if
    /// the uniqueness condition is violated condition it will cause incorrect
    /// behaviour but will not affect memory safety.
    #[inline]
    pub const fn new_unchecked(t: &'a T) -> Self {
        Interned(t, private::PrivateZst)
    }
}

impl<'a, T> Clone for Interned<'a, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T> Copy for Interned<'a, T> {}

impl<'a, T> Deref for Interned<'a, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.0
    }
}

impl<'a, T> PartialEq for Interned<'a, T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        // Pointer equality implies equality, due to the uniqueness constraint.
        ptr::eq(self.0, other.0)
    }
}

impl<'a, T> Eq for Interned<'a, T> {}

impl<'a, T: PartialOrd> PartialOrd for Interned<'a, T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // Pointer equality implies equality, due to the uniqueness constraint,
        // but the contents must be compared otherwise.
        if ptr::eq(self.0, other.0) {
            Some(Ordering::Equal)
        } else {
            let res = self.0.partial_cmp(other.0);
            debug_assert_ne!(res, Some(Ordering::Equal));
            res
        }
    }
}

impl<'a, T: Ord> Ord for Interned<'a, T> {
    fn cmp(&self, other: &Self) -> Ordering {
        // Pointer equality implies equality, due to the uniqueness constraint,
        // but the contents must be compared otherwise.
        if ptr::eq(self.0, other.0) {
            Ordering::Equal
        } else {
            let res = self.0.cmp(other.0);
            debug_assert_ne!(res, Ordering::Equal);
            res
        }
    }
}

impl<'a, T> Hash for Interned<'a, T> {
    #[inline]
    fn hash<H: Hasher>(&self, s: &mut H) {
        // Pointer hashing is sufficient, due to the uniqueness constraint.
        ptr::hash(self.0, s)
    }
}

impl<T: Debug> Debug for Interned<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct S(u32);

    impl PartialEq for S {
        fn eq(&self, _other: &Self) -> bool {
            panic!("shouldn't be called");
        }
    }

    impl Eq for S {}

    #[allow(clippy::non_canonical_partial_ord_impl)]
    impl PartialOrd for S {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            // The `==` case should be handled by `Interned`.
            assert_ne!(self.0, other.0);
            self.0.partial_cmp(&other.0)
        }
    }

    impl Ord for S {
        fn cmp(&self, other: &Self) -> Ordering {
            // The `==` case should be handled by `Interned`.
            assert_ne!(self.0, other.0);
            self.0.cmp(&other.0)
        }
    }

    #[test]
    fn test_uniq() {
        let s1 = S(1);
        let s2 = S(2);
        let s3 = S(3);
        let s4 = S(1); // violates uniqueness

        let v1 = Interned::new_unchecked(&s1);
        let v2 = Interned::new_unchecked(&s2);
        let v3a = Interned::new_unchecked(&s3);
        let v3b = Interned::new_unchecked(&s3);
        let v4 = Interned::new_unchecked(&s4); // violates uniqueness

        assert_ne!(v1, v2);
        assert_ne!(v2, v3a);
        assert_eq!(v1, v1);
        assert_eq!(v3a, v3b);
        assert_ne!(v1, v4); // same content but different addresses: not equal

        assert_eq!(v1.cmp(&v2), Ordering::Less);
        assert_eq!(v3a.cmp(&v2), Ordering::Greater);
        assert_eq!(v1.cmp(&v1), Ordering::Equal); // only uses Interned::eq, not S::cmp
        assert_eq!(v3a.cmp(&v3b), Ordering::Equal); // only uses Interned::eq, not S::cmp

        assert_eq!(v1.partial_cmp(&v2), Some(Ordering::Less));
        assert_eq!(v3a.partial_cmp(&v2), Some(Ordering::Greater));
        assert_eq!(v1.partial_cmp(&v1), Some(Ordering::Equal)); // only uses Interned::eq, not S::cmp
        assert_eq!(v3a.partial_cmp(&v3b), Some(Ordering::Equal)); // only uses Interned::eq, not
                                                                  // S::cmp
    }
}
