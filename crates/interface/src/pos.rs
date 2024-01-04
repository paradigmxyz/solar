use std::{
    fmt,
    ops::{Add, AddAssign, Sub, SubAssign},
};

pub trait Pos {
    fn from_usize(n: usize) -> Self;
    fn to_usize(&self) -> usize;
    fn from_u32(n: u32) -> Self;
    fn to_u32(&self) -> u32;
}

macro_rules! impl_pos {
    (
        $(
            $(#[$attr:meta])*
            $vis:vis struct $ident:ident($inner_vis:vis $inner_ty:ty);
        )*
    ) => {
        $(
            $(#[$attr])*
            $vis struct $ident($inner_vis $inner_ty);

            impl fmt::Debug for $ident {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    write!(f, "{}({})", stringify!($ident), self.0)
                }
            }

            impl Pos for $ident {
                #[inline(always)]
                fn from_usize(n: usize) -> Self {
                    Self(n as $inner_ty)
                }

                #[inline(always)]
                fn to_usize(&self) -> usize {
                    self.0 as usize
                }

                #[inline(always)]
                fn from_u32(n: u32) -> Self {
                    Self(n as $inner_ty)
                }

                #[inline(always)]
                fn to_u32(&self) -> u32 {
                    self.0 as u32
                }
            }

            impl Add for $ident {
                type Output = Self;

                #[inline(always)]
                fn add(self, rhs: Self) -> Self {
                    Self(self.0 + rhs.0)
                }
            }

            impl Add<$inner_ty> for $ident {
                type Output = Self;

                #[inline(always)]
                fn add(self, rhs: $inner_ty) -> Self {
                    Self(self.0 + rhs)
                }
            }

            impl AddAssign for $ident {
                #[inline(always)]
                fn add_assign(&mut self, rhs: Self) {
                    *self = Self(self.0 + rhs.0);
                }
            }

            impl AddAssign<$inner_ty> for $ident {
                #[inline(always)]
                fn add_assign(&mut self, rhs: $inner_ty) {
                    self.0 += rhs;
                }
            }

            impl Sub for $ident {
                type Output = Self;

                #[inline(always)]
                fn sub(self, rhs: Self) -> Self {
                    Self(self.0 - rhs.0)
                }
            }

            impl Sub<$inner_ty> for $ident {
                type Output = Self;

                #[inline(always)]
                fn sub(self, rhs: $inner_ty) -> Self {
                    Self(self.0 - rhs)
                }
            }

            impl SubAssign for $ident {
                #[inline(always)]
                fn sub_assign(&mut self, rhs: Self) {
                    *self = *self - rhs;
                }
            }

            impl SubAssign<$inner_ty> for $ident {
                #[inline(always)]
                fn sub_assign(&mut self, rhs: $inner_ty) {
                    self.0 -= rhs;
                }
            }
        )*
    };
}

impl_pos! {
    /// A byte offset.
    ///
    /// Keep this small (currently 32-bits), as AST contains a lot of them.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct BytePos(pub u32);

    /// A byte offset relative to file beginning.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct RelativeBytePos(pub u32);

    /// A character offset.
    ///
    /// Because of multibyte UTF-8 characters, a byte offset
    /// is not equivalent to a character offset. The [`SourceMap`](crate::SourceMap) will convert
    /// [`BytePos`] values to `CharPos` values as necessary.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct CharPos(pub usize);
}
