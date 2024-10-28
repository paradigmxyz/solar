use crate::{SessionGlobals, Span};
use solar_data_structures::{index::BaseIndex32, trustme};
use solar_macros::symbols;
use std::{cmp, fmt, hash, str};

/// An identifier.
#[derive(Clone, Copy)]
pub struct Ident {
    /// The identifier's name.
    pub name: Symbol,
    /// The identifier's span.
    pub span: Span,
}

impl Default for Ident {
    #[inline]
    fn default() -> Self {
        Self::DUMMY
    }
}

impl PartialEq for Ident {
    #[inline]
    fn eq(&self, rhs: &Self) -> bool {
        self.name == rhs.name
    }
}

impl Eq for Ident {}

impl PartialOrd for Ident {
    #[inline]
    fn partial_cmp(&self, rhs: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(rhs))
    }
}

impl Ord for Ident {
    #[inline]
    fn cmp(&self, rhs: &Self) -> cmp::Ordering {
        self.name.cmp(&rhs.name)
    }
}

impl hash::Hash for Ident {
    #[inline]
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl fmt::Debug for Ident {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for Ident {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.name.fmt(f)
    }
}

impl Ident {
    /// A dummy identifier.
    pub const DUMMY: Self = Self::new(Symbol::DUMMY, Span::DUMMY);

    /// Constructs a new identifier from a symbol and a span.
    #[inline]
    pub const fn new(name: Symbol, span: Span) -> Self {
        Self { name, span }
    }

    /// Constructs a new identifier with a dummy span.
    #[inline]
    pub const fn with_dummy_span(name: Symbol) -> Self {
        Self::new(name, Span::DUMMY)
    }

    /// Maps a string to an identifier with a dummy span.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(string: &str) -> Self {
        Self::with_dummy_span(Symbol::intern(string))
    }

    /// Maps a string and a span to an identifier.
    pub fn from_str_and_span(string: &str, span: Span) -> Self {
        Self::new(Symbol::intern(string), span)
    }

    /// "Specialization" of [`ToString`] using [`as_str`](Self::as_str).
    #[inline]
    #[allow(clippy::inherent_to_string_shadow_display)]
    pub fn to_string(&self) -> String {
        self.as_str().to_string()
    }

    /// Access the underlying string. This is a slowish operation because it requires locking the
    /// symbol interner.
    ///
    /// Note that the lifetime of the return value is a lie. See [`Symbol::as_str()`] for details.
    pub fn as_str(&self) -> &str {
        self.name.as_str()
    }

    /// Returns `true` if the identifier is a keyword used in the language.
    #[inline]
    pub fn is_used_keyword(self) -> bool {
        self.name.is_used_keyword()
    }

    /// Returns `true` if the identifier is a keyword reserved for possible future use.
    #[inline]
    pub fn is_unused_keyword(self) -> bool {
        self.name.is_unused_keyword()
    }

    /// Returns `true` if the identifier is a weak keyword and can be used in variable names.
    #[inline]
    pub fn is_weak_keyword(self) -> bool {
        self.name.is_weak_keyword()
    }

    /// Returns `true` if the identifier is a keyword in a Yul context.
    #[inline]
    pub fn is_yul_keyword(self) -> bool {
        self.name.is_yul_keyword()
    }

    /// Returns `true` if the identifier is a Yul EVM builtin keyword.
    #[inline]
    pub fn is_yul_evm_builtin(self) -> bool {
        self.name.is_yul_builtin()
    }

    /// Returns `true` if the identifier is either a keyword, either currently in use or reserved
    /// for possible future use.
    #[inline]
    pub fn is_reserved(self, yul: bool) -> bool {
        self.name.is_reserved(yul)
    }

    /// Returns `true` if the identifier is not a reserved keyword.
    /// See [`is_reserved`](Self::is_reserved).
    #[inline]
    pub fn is_non_reserved(self, yul: bool) -> bool {
        self.name.is_non_reserved(yul)
    }

    /// Returns `true` if the identifier is an elementary type name.
    ///
    /// Note that this does not include `[u]fixedMxN` types.
    #[inline]
    pub fn is_elementary_type(self) -> bool {
        self.name.is_elementary_type()
    }

    /// Returns `true` if the identifier is `true` or `false`.
    #[inline]
    pub fn is_bool_lit(self) -> bool {
        self.name.is_bool_lit()
    }

    /// Returns `true` if the identifier is a location specifier.
    #[inline]
    pub fn is_location_specifier(self) -> bool {
        self.name.is_location_specifier()
    }
}

/// An interned string.
///
/// Internally, a `Symbol` is implemented as an index, and all operations
/// (including hashing, equality, and ordering) operate on that index.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Symbol(BaseIndex32);

impl Default for Symbol {
    #[inline]
    fn default() -> Self {
        Self::DUMMY
    }
}

impl Symbol {
    /// A dummy symbol.
    pub const DUMMY: Self = kw::Empty;

    const fn new(n: u32) -> Self {
        Self(BaseIndex32::new(n))
    }

    /// Maps a string to its interned representation.
    pub fn intern(string: &str) -> Self {
        SessionGlobals::with(|g| g.symbol_interner.intern(string))
    }

    /// "Specialization" of [`ToString`] using [`as_str`](Self::as_str).
    #[inline]
    #[allow(clippy::inherent_to_string_shadow_display)]
    pub fn to_string(&self) -> String {
        self.as_str().to_string()
    }

    /// Access the underlying string. This is a slowish operation because it
    /// requires locking the symbol interner.
    ///
    /// Note that the lifetime of the return value is a lie. It's not the same
    /// as `&self`, but actually tied to the lifetime of the underlying
    /// interner. Interners are long-lived, and there are very few of them, and
    /// this function is typically used for short-lived things, so in practice
    /// it works out ok.
    pub fn as_str(&self) -> &str {
        SessionGlobals::with(|g| unsafe { trustme::decouple_lt(g.symbol_interner.get(*self)) })
    }

    /// Returns the internal representation of the symbol.
    #[inline(always)]
    pub const fn as_u32(self) -> u32 {
        self.0.get()
    }

    /// Returns `true` if the symbol is a keyword used in the Solidity language.
    ///
    /// For Yul keywords, use [`is_yul_keyword`](Self::is_yul_keyword).
    #[inline]
    pub fn is_used_keyword(self) -> bool {
        self < kw::After
    }

    /// Returns `true` if the symbol is a keyword reserved for possible future use.
    #[inline]
    pub fn is_unused_keyword(self) -> bool {
        self >= kw::After && self <= kw::Var
    }

    /// Returns `true` if the symbol is a weak keyword and can be used in variable names.
    #[inline]
    pub fn is_weak_keyword(self) -> bool {
        self >= kw::Leave && self <= kw::Builtin
    }

    /// Returns `true` if the symbol is a keyword in a Yul context. Excludes EVM builtins.
    #[inline]
    pub fn is_yul_keyword(self) -> bool {
        // https://github.com/ethereum/solidity/blob/194b114664c7daebc2ff68af3c573272f5d28913/liblangutil/Token.h#L329
        matches!(
            self,
            kw::Function
                | kw::Let
                | kw::If
                | kw::Switch
                | kw::Case
                | kw::Default
                | kw::For
                | kw::Break
                | kw::Continue
                | kw::Leave
                | kw::True
                | kw::False
        )
    }

    /// Returns `true` if the symbol is a Yul EVM builtin keyword.
    #[inline]
    pub fn is_yul_builtin(self) -> bool {
        (self >= kw::Add && self <= kw::Xor)
            | matches!(self, kw::Address | kw::Byte | kw::Return | kw::Revert)
    }

    /// Returns `true` if the symbol is either a keyword, either currently in use or reserved for
    /// possible future use.
    #[inline]
    pub fn is_reserved(self, yul: bool) -> bool {
        if yul {
            self.is_yul_keyword() | self.is_yul_builtin()
        } else {
            self.is_used_keyword() | self.is_unused_keyword()
        }
    }

    /// Returns `true` if the symbol is not a reserved keyword.
    /// See [`is_reserved`](Self::is_reserved).
    #[inline]
    pub fn is_non_reserved(self, yul: bool) -> bool {
        !self.is_reserved(yul)
    }

    /// Returns `true` if the symbol is an elementary type name.
    ///
    /// Note that this does not include `[u]fixedMxN` types as they are not pre-interned.
    #[inline]
    pub fn is_elementary_type(self) -> bool {
        self >= kw::Int && self <= kw::UFixed
    }

    /// Returns `true` if the symbol is `true` or `false`.
    #[inline]
    pub fn is_bool_lit(self) -> bool {
        self == kw::False || self == kw::True
    }

    /// Returns `true` if the symbol is a location specifier.
    #[inline]
    pub fn is_location_specifier(self) -> bool {
        matches!(self, kw::Calldata | kw::Memory | kw::Storage)
    }

    /// Returns `true` if the symbol was interned in the compiler's `symbols!` macro.
    #[inline]
    pub const fn is_preinterned(self) -> bool {
        self.as_u32() < PREINTERNED_SYMBOLS_COUNT
    }
}

impl fmt::Debug for Symbol {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for Symbol {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}

type InternerInner = LassoInterner;

/// Symbol interner.
///
/// Initialized in `SessionGlobals` with the `symbols!` macro's initial symbols.
pub(crate) struct Interner(InternerInner);

impl Interner {
    pub(crate) fn fresh() -> Self {
        Self(InternerInner::fresh())
    }

    #[cfg(test)]
    pub(crate) fn prefill(init: &[&'static str]) -> Self {
        Self(InternerInner::prefill(init))
    }

    #[inline]
    fn intern(&self, string: &str) -> Symbol {
        self.0.intern(string)
    }

    #[inline]
    fn get(&self, symbol: Symbol) -> &str {
        self.0.get(symbol)
    }
}

// TODO: We could finalize the interner after parsing to a `RodeoResolver`, making it read-only.
struct LassoInterner(lasso::ThreadedRodeo<Symbol, solar_data_structures::map::FxBuildHasher>);

impl LassoInterner {
    fn prefill(init: &[&'static str]) -> Self {
        let capacity = if init.is_empty() {
            Default::default()
        } else {
            let actual_string = init.len();
            let strings = actual_string.next_power_of_two();
            let actual_bytes = PREINTERNED_SYMBOLS_BYTES as usize;
            let bytes = actual_bytes.next_power_of_two().max(4096);
            trace!(strings, bytes, "prefill capacity");
            lasso::Capacity::new(strings, std::num::NonZeroUsize::new(bytes).unwrap())
        };
        let rodeo = lasso::ThreadedRodeo::with_capacity_and_hasher(capacity, Default::default());
        for &s in init {
            rodeo.get_or_intern_static(s);
        }
        Self(rodeo)
    }

    #[inline]
    fn intern(&self, string: &str) -> Symbol {
        self.0.get_or_intern(string)
    }

    #[inline]
    fn get(&self, symbol: Symbol) -> &str {
        self.0.resolve(&symbol)
    }
}

unsafe impl lasso::Key for Symbol {
    #[inline]
    fn into_usize(self) -> usize {
        self.as_u32() as usize
    }

    #[inline]
    fn try_from_usize(int: usize) -> Option<Self> {
        int.try_into().ok().map(Self::new)
    }
}

// This module has a very short name because it's used a lot.
/// This module contains all the defined keyword `Symbol`s.
///
/// Given that `kw` is imported, use them like `kw::keyword_name`.
/// For example `kw::For` or `kw::Break`.
pub mod kw {
    use crate::Symbol;

    #[doc(inline)]
    pub use super::kw_generated::*;

    /// Returns the boolean keyword for the given value.
    #[inline]
    pub const fn boolean(b: bool) -> Symbol {
        if b {
            True
        } else {
            False
        }
    }

    /// Returns the `int` keyword for the given byte (**not bit**) size.
    ///
    /// If `n` is 0, returns [`kw::Uint`](Int).
    ///
    /// # Panics
    ///
    /// Panics if `n` is greater than 32.
    #[inline]
    #[track_caller]
    pub const fn int(n: u8) -> Symbol {
        assert!(n <= 32);
        Symbol::new(Int.as_u32() + n as u32)
    }

    /// Returns the `uint` keyword for the given byte (**not bit**) size.
    ///
    /// If `n` is 0, returns [`kw::UInt`](UInt).
    ///
    /// # Panics
    ///
    /// Panics if `n` is greater than 32.
    #[inline]
    #[track_caller]
    pub const fn uint(n: u8) -> Symbol {
        assert!(n <= 32);
        Symbol::new(UInt.as_u32() + n as u32)
    }

    /// Returns the `bytes` keyword for the given byte size.
    ///
    /// # Panics
    ///
    /// Panics if `n` is 0 or is greater than 32.
    #[inline]
    #[track_caller]
    pub const fn fixed_bytes(n: u8) -> Symbol {
        assert!(n > 0 && n <= 32);
        Symbol::new(Bytes.as_u32() + n as u32)
    }
}

// This module has a very short name because it's used a lot.
/// This module contains all the defined non-keyword `Symbol`s.
///
/// Given that `sym` is imported, use them like `sym::symbol_name`.
/// For example `sym::rustfmt` or `sym::u8`.
pub mod sym {
    use super::Symbol;

    #[doc(inline)]
    pub use super::sym_generated::*;

    /// Get the symbol for an integer.
    ///
    /// The first few non-negative integers each have a static symbol and therefore are fast.
    pub fn integer<N: TryInto<usize> + Copy + itoa::Integer>(n: N) -> Symbol {
        if let Ok(idx @ 0..=9) = n.try_into() {
            return Symbol::new(super::SYMBOL_DIGITS_BASE + idx as u32);
        }
        Symbol::intern(itoa::Buffer::new().format(n))
    }
}

// The proc macro code for this is in `crates/macros/src/symbols/mod.rs`.
symbols! {
    // Solidity keywords.
    // When modifying this, also update all the `is_keyword` functions in this file.
    // Modified from the `TOKEN_LIST` macro in Solc: https://github.com/ethereum/solidity/blob/194b114664c7daebc2ff68af3c573272f5d28913/liblangutil/Token.h#L67
    Keywords {
        // Special symbols used internally.
        Empty:       "",

        Abstract:    "abstract",
        Anonymous:   "anonymous",
        As:          "as",
        Assembly:    "assembly",
        Break:       "break",
        Calldata:    "calldata",
        Catch:       "catch",
        Constant:    "constant",
        Constructor: "constructor",
        Continue:    "continue",
        Contract:    "contract",
        Delete:      "delete",
        Do:          "do",
        Else:        "else",
        Emit:        "emit",
        Enum:        "enum",
        Event:       "event",
        External:    "external",
        Fallback:    "fallback",
        For:         "for",
        Function:    "function",
        Hex:         "hex",
        If:          "if",
        Immutable:   "immutable",
        Import:      "import",
        Indexed:     "indexed",
        Interface:   "interface",
        Internal:    "internal",
        Is:          "is",
        Library:     "library",
        Mapping:     "mapping",
        Memory:      "memory",
        Modifier:    "modifier",
        New:         "new",
        Override:    "override",
        Payable:     "payable",
        Pragma:      "pragma",
        Private:     "private",
        Public:      "public",
        Pure:        "pure",
        Receive:     "receive",
        Return:      "return",
        Returns:     "returns",
        Storage:     "storage",
        Struct:      "struct",
        Throw:       "throw",
        Try:         "try",
        Type:        "type",
        Unchecked:   "unchecked",
        Unicode:     "unicode",
        Using:       "using",
        View:        "view",
        Virtual:     "virtual",
        While:       "while",

        // Types.
        Int:         "int",
        Int8:        "int8",
        Int16:       "int16",
        Int24:       "int24",
        Int32:       "int32",
        Int40:       "int40",
        Int48:       "int48",
        Int56:       "int56",
        Int64:       "int64",
        Int72:       "int72",
        Int80:       "int80",
        Int88:       "int88",
        Int96:       "int96",
        Int104:      "int104",
        Int112:      "int112",
        Int120:      "int120",
        Int128:      "int128",
        Int136:      "int136",
        Int144:      "int144",
        Int152:      "int152",
        Int160:      "int160",
        Int168:      "int168",
        Int176:      "int176",
        Int184:      "int184",
        Int192:      "int192",
        Int200:      "int200",
        Int208:      "int208",
        Int216:      "int216",
        Int224:      "int224",
        Int232:      "int232",
        Int240:      "int240",
        Int248:      "int248",
        Int256:      "int256",
        UInt:        "uint",
        UInt8:       "uint8",
        UInt16:      "uint16",
        UInt24:      "uint24",
        UInt32:      "uint32",
        UInt40:      "uint40",
        UInt48:      "uint48",
        UInt56:      "uint56",
        UInt64:      "uint64",
        UInt72:      "uint72",
        UInt80:      "uint80",
        UInt88:      "uint88",
        UInt96:      "uint96",
        UInt104:     "uint104",
        UInt112:     "uint112",
        UInt120:     "uint120",
        UInt128:     "uint128",
        UInt136:     "uint136",
        UInt144:     "uint144",
        UInt152:     "uint152",
        UInt160:     "uint160",
        UInt168:     "uint168",
        UInt176:     "uint176",
        UInt184:     "uint184",
        UInt192:     "uint192",
        UInt200:     "uint200",
        UInt208:     "uint208",
        UInt216:     "uint216",
        UInt224:     "uint224",
        UInt232:     "uint232",
        UInt240:     "uint240",
        UInt248:     "uint248",
        UInt256:     "uint256",
        Bytes:       "bytes",
        Bytes1:      "bytes1",
        Bytes2:      "bytes2",
        Bytes3:      "bytes3",
        Bytes4:      "bytes4",
        Bytes5:      "bytes5",
        Bytes6:      "bytes6",
        Bytes7:      "bytes7",
        Bytes8:      "bytes8",
        Bytes9:      "bytes9",
        Bytes10:     "bytes10",
        Bytes11:     "bytes11",
        Bytes12:     "bytes12",
        Bytes13:     "bytes13",
        Bytes14:     "bytes14",
        Bytes15:     "bytes15",
        Bytes16:     "bytes16",
        Bytes17:     "bytes17",
        Bytes18:     "bytes18",
        Bytes19:     "bytes19",
        Bytes20:     "bytes20",
        Bytes21:     "bytes21",
        Bytes22:     "bytes22",
        Bytes23:     "bytes23",
        Bytes24:     "bytes24",
        Bytes25:     "bytes25",
        Bytes26:     "bytes26",
        Bytes27:     "bytes27",
        Bytes28:     "bytes28",
        Bytes29:     "bytes29",
        Bytes30:     "bytes30",
        Bytes31:     "bytes31",
        Bytes32:     "bytes32",
        String:      "string",
        Address:     "address",
        Bool:        "bool",
        Fixed:       "fixed",
        UFixed:      "ufixed",

        // Number subdenominations.
        Wei:         "wei",
        Gwei:        "gwei",
        Ether:       "ether",
        Seconds:     "seconds",
        Minutes:     "minutes",
        Hours:       "hours",
        Days:        "days",
        Weeks:       "weeks",
        Years:       "years",

        // Literals.
        False:       "false",
        True:        "true",

        // Reserved for future use.
        After:       "after",
        Alias:       "alias",
        Apply:       "apply",
        Auto:        "auto",
        Byte:        "byte",
        Case:        "case",
        CopyOf:      "copyof",
        Default:     "default",
        Define:      "define",
        Final:       "final",
        Implements:  "implements",
        In:          "in",
        Inline:      "inline",
        Let:         "let",
        Macro:       "macro",
        Match:       "match",
        Mutable:     "mutable",
        NullLiteral: "null",
        Of:          "of",
        Partial:     "partial",
        Promise:     "promise",
        Reference:   "reference",
        Relocatable: "relocatable",
        Sealed:      "sealed",
        Sizeof:      "sizeof",
        Static:      "static",
        Supports:    "supports",
        Switch:      "switch",
        Typedef:     "typedef",
        TypeOf:      "typeof",
        Var:         "var",

        // All the following keywords are 'weak' keywords,
        // which means they can be used as variable names.

        // Yul specific keywords.
        Leave:       "leave",
        Revert:      "revert",

        // Yul EVM builtins.
        // Some builtins have already been previously declared, so they can't be redeclared here.
        // See `is_yul_builtin`.
        // https://docs.soliditylang.org/en/latest/yul.html#evm-dialect
        // TODO: The remaining internal dialect builtins.
        Add:            "add",
        Addmod:         "addmod",
        And:            "and",
        Balance:        "balance",
        Basefee:        "basefee",
        Blobbasefee:    "blobbasefee",
        Blobhash:       "blobhash",
        Blockhash:      "blockhash",
        Call:           "call",
        Callcode:       "callcode",
        Calldatacopy:   "calldatacopy",
        Calldataload:   "calldataload",
        Calldatasize:   "calldatasize",
        Caller:         "caller",
        Callvalue:      "callvalue",
        Chainid:        "chainid",
        Coinbase:       "coinbase",
        Create:         "create",
        Create2:        "create2",
        Delegatecall:   "delegatecall",
        Difficulty:     "difficulty",
        Div:            "div",
        Eq:             "eq",
        Exp:            "exp",
        Extcodecopy:    "extcodecopy",
        Extcodehash:    "extcodehash",
        Extcodesize:    "extcodesize",
        Gas:            "gas",
        Gaslimit:       "gaslimit",
        Gasprice:       "gasprice",
        Gt:             "gt",
        Invalid:        "invalid",
        Iszero:         "iszero",
        Keccak256:      "keccak256",
        Log0:           "log0",
        Log1:           "log1",
        Log2:           "log2",
        Log3:           "log3",
        Log4:           "log4",
        Lt:             "lt",
        Mcopy:          "mcopy",
        Mload:          "mload",
        Mod:            "mod",
        Msize:          "msize",
        Mstore:         "mstore",
        Mstore8:        "mstore8",
        Mul:            "mul",
        Mulmod:         "mulmod",
        Not:            "not",
        Number:         "number",
        Or:             "or",
        Origin:         "origin",
        Pop:            "pop",
        Prevrandao:     "prevrandao",
        Returndatacopy: "returndatacopy",
        Returndatasize: "returndatasize",
        Sar:            "sar",
        Sdiv:           "sdiv",
        Selfbalance:    "selfbalance",
        Selfdestruct:   "selfdestruct",
        Sgt:            "sgt",
        Shl:            "shl",
        Shr:            "shr",
        Signextend:     "signextend",
        Sload:          "sload",
        Slt:            "slt",
        Smod:           "smod",
        Sstore:         "sstore",
        Staticcall:     "staticcall",
        Stop:           "stop",
        Sub:            "sub",
        Timestamp:      "timestamp",
        Tload:          "tload",
        Tstore:         "tstore",
        Xor:            "xor",

        // Experimental Solidity specific keywords.
        Class:         "class",
        Instantiation: "instantiation",
        Integer:       "Integer",
        Itself:        "itself",
        StaticAssert:  "static_assert",
        Builtin:       "__builtin",
        ForAll:        "forall",
    }

    // Pre-interned symbols that can be referred to with `sym::*`.
    //
    // The symbol is the stringified identifier unless otherwise specified, in
    // which case the name should mention the non-identifier punctuation.
    // E.g. `sym::proc_dash_macro` represents "proc-macro", and it shouldn't be
    // called `sym::proc_macro` because then it's easy to mistakenly think it
    // represents "proc_macro".
    //
    // As well as the symbols listed, there are symbols for the strings
    // "0", "1", ..., "9", which are accessible via `sym::integer`.
    //
    // The proc macro will abort if symbols are not in alphabetical order (as
    // defined by `impl Ord for str`) or if any symbols are duplicated. Vim
    // users can sort the list by selecting it and executing the command
    // `:'<,'>!LC_ALL=C sort`.
    //
    // There is currently no checking that all symbols are used; that would be
    // nice to have.
    Symbols {
        X,
        __tmp_struct,
        abi,
        abicoder,
        assert,
        block,
        code,
        codehash,
        concat,
        creationCode,
        data,
        decode,
        ecrecover,
        encode,
        encodeCall,
        encodePacked,
        encodeWithSelector,
        encodeWithSignature,
        error,
        experimental,
        from,
        global,
        interfaceId,
        length,
        max,
        min,
        msg,
        name,
        object,
        push,
        require,
        ripemd160,
        runtimeCode,
        selector,
        send,
        sender,
        sha256,
        sig,
        solidity,
        super_: "super",
        this,
        transfer,
        transient,
        tx,
        underscore: "_",
        unwrap,
        value,
        wrap,
        x,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interner_tests() {
        let i = Interner::prefill(&[]);
        // first one is zero:
        assert_eq!(i.intern("dog"), Symbol::new(0));
        // re-use gets the same entry:
        assert_eq!(i.intern("dog"), Symbol::new(0));
        // different string gets a different #:
        assert_eq!(i.intern("cat"), Symbol::new(1));
        assert_eq!(i.intern("cat"), Symbol::new(1));
        // dog is still at zero
        assert_eq!(i.intern("dog"), Symbol::new(0));
    }

    #[test]
    fn defaults() {
        assert_eq!(Symbol::DUMMY, Symbol::new(0));
        assert_eq!(Symbol::DUMMY, Symbol::default());
        assert_eq!(Ident::DUMMY, Ident::new(Symbol::DUMMY, Span::DUMMY));
        assert_eq!(Ident::DUMMY, Ident::default());

        crate::enter(|| {
            assert_eq!(Symbol::DUMMY.as_str(), "");
            assert_eq!(Symbol::DUMMY.to_string(), "");
            assert_eq!(Ident::DUMMY.as_str(), "");
            assert_eq!(Ident::DUMMY.to_string(), "");
        });
    }
}
