use crate::with_session_globals;
use rsolc_data_structures::{fx::FxHashMap, DroplessArena};
use rsolc_macros::symbols;
use std::{cell::RefCell, fmt, str};

symbols! {
    Keywords {

    }

    Symbols {

    }
}

/// An interned string.
///
/// Internally, a `Symbol` is implemented as an index, and all operations
/// (including hashing, equality, and ordering) operate on that index. The use
/// of `rustc_index::newtype_index!` means that `Option<Symbol>` only takes up 4 bytes,
/// because `rustc_index::newtype_index!` reserves the last 256 values for tagging purposes.
///
/// Note that `Symbol` cannot directly be a `rustc_index::newtype_index!` because it
/// implements `fmt::Debug`, `Encodable`, and `Decodable` in special ways.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Symbol(u32);

impl Symbol {
    #[inline]
    const fn new(n: u32) -> Self {
        Symbol(n)
    }

    /// Maps a string to its interned representation.
    pub fn intern(string: &str) -> Self {
        with_session_globals(|session_globals| session_globals.symbol_interner.intern(string))
    }

    /// Access the underlying string.
    ///
    /// Note that the lifetime of the return value is a lie. It's not the same
    /// as `&self`, but actually tied to the lifetime of the underlying
    /// interner. Interners are long-lived, and there are very few of them, and
    /// this function is typically used for short-lived things, so in practice
    /// it works out ok.
    pub fn as_str(&self) -> &str {
        with_session_globals(|session_globals| unsafe {
            std::mem::transmute::<&str, &str>(session_globals.symbol_interner.get(*self))
        })
    }

    #[inline]
    pub fn as_u32(self) -> u32 {
        self.0
    }

    // pub fn is_empty(self) -> bool {
    //     self == kw::Empty
    // }

    // /// This method is supposed to be used in error messages, so it's expected to be
    // /// identical to printing the original identifier token written in source code
    // /// (`token_to_string`, `Ident::to_string`), except that symbols don't keep the rawness flag
    // /// or edition, so we have to guess the rawness using the global edition.
    // pub fn to_ident_string(self) -> String {
    //     Ident::with_dummy_span(self).to_string()
    // }
}

impl fmt::Debug for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}

pub(crate) struct Interner(RefCell<InternerInner>);

// The `&'static str`s in this type actually point into the arena.
//
// The `FxHashMap`+`Vec` pair could be replaced by `FxIndexSet`, but #75278
// found that to regress performance up to 2% in some cases. This might be
// revisited after further improvements to `indexmap`.
//
// This type is private to prevent accidentally constructing more than one
// `Interner` on the same thread, which makes it easy to mix up `Symbol`s
// between `Interner`s.
struct InternerInner {
    arena: DroplessArena,
    names: FxHashMap<&'static str, Symbol>,
    strings: Vec<&'static str>,
}

impl Interner {
    fn prefill(init: &[&'static str]) -> Self {
        Interner(RefCell::new(InternerInner {
            arena: Default::default(),
            names: init.iter().copied().zip((0..).map(Symbol::new)).collect(),
            strings: init.to_vec(),
        }))
    }

    #[inline]
    fn intern(&self, string: &str) -> Symbol {
        let inner = self.0.borrow_mut();
        if let Some(&name) = inner.names.get(string) {
            return name;
        }

        let name = Symbol::new(inner.strings.len() as u32);

        // SAFETY: we convert from `&str` to `&[u8]`, clone it into the arena,
        // and immediately convert the clone back to `&[u8]`, all because there
        // is no `inner.arena.alloc_str()` method. This is clearly safe.
        let string: &str =
            unsafe { str::from_utf8_unchecked(inner.arena.alloc_slice(string.as_bytes())) };

        // SAFETY: we can extend the arena allocation to `'static` because we
        // only access these while the arena is still alive.
        let mut inner = self.0.borrow_mut();
        let string: &'static str = unsafe { &*(string as *const str) };
        inner.strings.push(string);

        // This second hash table lookup can be avoided by using `RawEntryMut`,
        // but this code path isn't hot enough for it to be worth it. See
        // #91445 for details.
        inner.names.insert(string, name);
        name
    }

    // Get the symbol as a string. `Symbol::as_str()` should be used in
    // preference to this function.
    fn get(&self, symbol: Symbol) -> &str {
        self.0.borrow().strings[symbol.0 as usize]
    }
}
