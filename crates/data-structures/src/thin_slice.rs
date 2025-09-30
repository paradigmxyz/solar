use bumpalo::Bump;
use std::{alloc::Layout, fmt};

// Modified from [`rustc_middle::ty::List`](https://github.com/rust-lang/rust/blob/a2db9280539229a3b8a084a09886670a57bc7e9c/compiler/rustc_middle/src/ty/list.rs#L15).

/// A thin-pointer slice of `T`.
///
/// This is similar to `[T]`, but the length is stored in the slice itself,
/// rather than in a separate field, making it a single word in size, instead of two.
pub type ThinSlice<T> = RawThinSlice<(), T>;

/// [`ThinSlice`] with a custom header.
pub struct RawThinSlice<H, T> {
    skel: ThinSliceSkeleton<H, T>,
    _opaque: OpaqueListContents,
}

#[repr(C)]
struct ThinSliceSkeleton<H, T> {
    header: H,
    len: usize,
    /// Although this claims to be a zero-length array, in practice `len`
    /// elements are actually present.
    data: [T; 0],
}

// Makes `RawThinSlice` unsized. Unfortunately only available on nightly.
#[cfg(not(feature = "nightly"))]
type OpaqueListContents = ();
#[cfg(feature = "nightly")]
unsafe extern "C" {
    type OpaqueListContents;
}

impl<H, T> RawThinSlice<H, T> {
    /// Returns a reference to the header.
    #[inline]
    pub fn header(&self) -> &H {
        &self.skel.header
    }

    /// Returns a mutable reference to the header.
    #[inline]
    pub fn header_mut(&mut self) -> &mut H {
        &mut self.skel.header
    }

    /// Allocates a list from `arena` and copies the contents of `slice` into it.
    #[inline]
    #[expect(clippy::mut_from_ref)] // Arena.
    pub(super) fn from_arena<'a>(arena: &'a Bump, header: H, slice: &[T]) -> &'a mut Self {
        let mem = Self::alloc_from_arena(arena, slice.len());
        // SAFETY: `mem` comes from `alloc_from_arena`.
        unsafe { Self::init(mem, header, slice) }
    }

    /// Allocates a list from `arena` and calls `f` for each element.
    #[inline]
    #[expect(clippy::mut_from_ref)] // Arena.
    pub(super) fn from_arena_with(
        arena: &Bump,
        header: H,
        len: usize,
        f: impl FnMut(usize) -> T,
    ) -> &mut Self {
        let mem = Self::alloc_from_arena(arena, len);
        // SAFETY: `mem` comes from `alloc_from_arena`.
        unsafe { Self::init_with(mem, header, len, f) }
    }

    /// Allocates a list from `arena`.
    #[inline]
    fn alloc_from_arena(arena: &Bump, len: usize) -> *mut Self {
        let (layout, _offset) = Layout::new::<ThinSliceSkeleton<H, T>>()
            .extend(Layout::array::<T>(len).unwrap())
            .unwrap();
        arena.alloc_layout(layout).as_ptr() as *mut Self
    }

    /// Initializes a list by copying the contents of `slice` into it.
    ///
    /// # Safety
    ///
    /// `mem` must come from `alloc_from_arena`.
    #[inline]
    unsafe fn init<'a>(mem: *mut Self, header: H, slice: &[T]) -> &'a mut Self {
        unsafe {
            (&raw mut (*mem).skel.header).write(header);
            (&raw mut (*mem).skel.len).write(slice.len());
            (&raw mut (*mem).skel.data)
                .cast::<T>()
                .copy_from_nonoverlapping(slice.as_ptr(), slice.len());
            &mut *mem
        }
    }

    /// Initializes a list by calling `f` for each element.
    ///
    /// # Safety
    ///
    /// `mem` must come from `alloc_from_arena`.
    #[inline]
    unsafe fn init_with<'a>(
        mem: *mut Self,
        header: H,
        len: usize,
        mut f: impl FnMut(usize) -> T,
    ) -> &'a mut Self {
        unsafe {
            (&raw mut (*mem).skel.header).write(header);
            (&raw mut (*mem).skel.len).write(len);
            for i in 0..len {
                (&raw mut (*mem).skel.data).cast::<T>().add(i).write(f(i));
            }
            &mut *mem
        }
    }

    /// Returns the number of elements in the slice.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.skel.len
    }

    /// Returns `true` if the slice is empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.skel.len == 0
    }

    /// Returns the slice as a slice of `T`.
    #[inline(always)]
    pub fn as_slice(&self) -> &[T] {
        self
    }

    /// Returns the slice as a mutable slice of `T`.
    #[inline(always)]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        self
    }
}

unsafe impl<H: Send, T: Send> Send for RawThinSlice<H, T> {}
unsafe impl<H: Sync, T: Sync> Sync for RawThinSlice<H, T> {}

impl<H, T> std::ops::Deref for RawThinSlice<H, T> {
    type Target = [T];

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        let data_ptr = (&raw const self.skel.data).cast::<T>();
        // SAFETY: `data_ptr` has the same provenance as `self` and can therefore
        // access the `self.skel.len` elements stored at `self.skel.data`.
        // Note that we specifically don't reborrow `&self.skel.data`, because that
        // would give us a pointer with provenance over 0 bytes.
        unsafe { std::slice::from_raw_parts(data_ptr, self.len()) }
    }
}

impl<H, T> std::ops::DerefMut for RawThinSlice<H, T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        let data_ptr = (&raw mut self.skel.data).cast::<T>();
        // SAFETY: See `Deref`.
        unsafe { std::slice::from_raw_parts_mut(data_ptr, self.len()) }
    }
}

impl<T> Default for &RawThinSlice<(), T> {
    /// Returns a reference to the (per header unique, static) empty slice.
    #[inline(always)]
    fn default() -> Self {
        assert!(align_of::<T>() <= align_of::<MaxAlign>());

        // SAFETY: `EMPTY` is sufficiently aligned to be an empty slice for all
        // types with `align_of(T) <= align_of(MaxAlign)`, which we checked above.
        unsafe { &*((&raw const EMPTY) as *const Self) }
    }
}

impl<T> Default for &mut RawThinSlice<(), T> {
    /// Returns a reference to the (per header unique, static) empty slice.
    #[inline(always)]
    fn default() -> Self {
        assert!(align_of::<T>() <= align_of::<MaxAlign>());

        // SAFETY: `EMPTY` is sufficiently aligned to be an empty slice for all
        // types with `align_of(T) <= align_of(MaxAlign)`, which we checked above.
        unsafe { &mut *((&raw mut EMPTY) as *mut Self) }
    }
}

#[repr(align(64))]
struct MaxAlign;

// `mut` but nothing inside can ever be mutated. `header` is ZST, `len` and `data` are exposed as an
// empty `&mut []`.
static mut EMPTY: ThinSliceSkeleton<(), MaxAlign> =
    ThinSliceSkeleton { header: (), len: 0, data: [] };

impl<H, T: fmt::Debug> fmt::Debug for RawThinSlice<H, T> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

impl<H, T: PartialEq> PartialEq for RawThinSlice<H, T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<H, T: Eq> Eq for RawThinSlice<H, T> {}

impl<H, T: PartialOrd> PartialOrd for RawThinSlice<H, T> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.as_slice().partial_cmp(other.as_slice())
    }
}

impl<H, T: Ord> Ord for RawThinSlice<H, T> {
    #[inline]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_slice().cmp(other.as_slice())
    }
}

impl<H, T: std::hash::Hash> std::hash::Hash for RawThinSlice<H, T> {
    #[inline]
    fn hash<Hasher: std::hash::Hasher>(&self, state: &mut Hasher) {
        self.as_slice().hash(state)
    }
}

impl<'a, H, T> IntoIterator for &'a RawThinSlice<H, T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, H, T> IntoIterator for &'a mut RawThinSlice<H, T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}
