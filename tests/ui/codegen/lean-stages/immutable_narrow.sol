//@ revisions: gas size
//@ compile-flags: --emit=bin-runtime -Zemit-lean-stages=../../target/ui-lean-stages
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
// Immutables narrower than a word export with their placeholders: an address, a `uint8` and a
// `bool` read the bytes of their `PUSH20` or `PUSH1` as they are, and, where the code pushes
// fewer bytes than a word for them (`size`), an `int16` sign-extends its `PUSH2` and a
// `bytes4` shifts its `PUSH4` to the left of the word. The manifest lists every placeholder.

contract LeanStagesNarrowImmutables {
    address immutable a;
    uint8 immutable u;
    int16 immutable i;
    bytes4 immutable b;
    bool immutable flag;

    constructor(address a_, uint8 u_, int16 i_, bytes4 b_, bool flag_) {
        a = a_;
        u = u_;
        i = i_;
        b = b_;
        flag = flag_;
    }

    function f() external view returns (address, uint8, int16, bytes4, bool) {
        return (a, u, i, b, flag);
    }
}
