//@ codegen-matrix: standard
//@ run-call: Modifiers::guarded 3 => 4
//@ run-call-fail: Modifiers::guarded 11 => 0x08c379a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000056775617264000000000000000000000000000000000000000000000000000000
//@ run-call: Modifiers::orderFull 0 => 1110221032919
//@ run-call: Modifiers::double 5 => 10
//@ run-call: Modifiers::maybe true => 0
//@ run-call: Modifiers::maybe false => 7
//@ run-call: Modifiers::retMid => 9
//@ run-call: Modifiers::lazy true => 0
//@ run-call: Modifiers::clamped 9 => 5
//@ run-call: Modifiers::clamped 2 => 2
//@ run-call: Modifiers::repeated => 212111
//@ run-call: Modifiers::literalString => "abc"
//@ run-call: Modifiers::literalBytes => 0x11223344
//@ run-call: Modifiers::calldataBytes 0x7061796c6f6164 => 0x7061796c6f6164
//@ run-call: ModBase::guardedV => 1
//@ run-call: ModDerived::guardedV => 100
//@ run-call: CtorMod::v => 50

// Function modifiers inline around the body, solc-style: arguments evaluate
// lazily per level, each placeholder re-runs the inner chain, a `return`
// inside a modifier leaves only that modifier, and a body `return` still runs
// the post-placeholder modifier code with its value preserved.

contract Modifiers {
    uint256 internal t;

    function rec(uint256 d) internal returns (uint256) {
        t = t * 100 + d;
        return d;
    }

    modifier guard(uint256 x) {
        require(x < 10, "guard");
        _;
    }

    // Unnamed return through a modifier: the body's value must survive the
    // chain exit.
    function guarded(uint256 x) public pure guard(x) returns (uint256) {
        return x + 1;
    }

    function literalString() public pure guard(0) returns (string memory) {
        return "abc";
    }

    function literalBytes() public pure guard(0) returns (bytes4) {
        return 0x11223344;
    }

    function calldataBytes(bytes calldata value) external pure guard(0) returns (bytes memory) {
        return value;
    }

    modifier mA(uint256 x) {
        rec(11);
        _;
        rec(19);
    }

    modifier mB(uint256 x) {
        rec(21);
        _;
        rec(29);
    }

    function inner(uint256 v) internal mA(rec(1)) mB(rec(2)) {
        rec(3);
    }

    // Digit trace of the full chain: mA arg, mA pre, mB arg, mB pre, body,
    // mB post, mA post.
    function orderFull(uint256 v) public returns (uint256) {
        inner(v);
        return t;
    }

    modifier twice() {
        _;
        _;
    }

    // Both placeholders run the body; the last `return` wins.
    function double(uint256 v) public twice returns (uint256) {
        t += v;
        return t;
    }

    modifier skipIf(bool c) {
        if (c) {
            return;
        }
        _;
    }

    // A modifier `return` before `_` skips the body; returns stay default.
    function maybe(bool c) public pure skipIf(c) returns (uint256 out) {
        out = 7;
    }

    modifier check1001() {
        _;
        require(t == 1001, "post");
    }

    modifier bump() {
        _;
        t += 1000;
    }

    // The body's `return 9` must still run `bump`'s post-code (verified by
    // `check1001`) without clobbering the returned value.
    function retMid() public check1001 bump returns (uint256) {
        t = 1;
        return 9;
    }

    modifier expectT0() {
        _;
        require(t == 0, "lazy");
    }

    // `skipIf(true)` returns before its placeholder, so `mA`'s argument must
    // never be evaluated; eager evaluation would set `t` and fail `expectT0`.
    function lazy(bool c) public expectT0 skipIf(c) mA(rec(7)) returns (uint256 out) {
        out = 5;
    }

    modifier clamp(uint256 lim) {
        if (lim > 5) {
            lim = 5;
        }
        t = lim;
        _;
    }

    // A reassigned modifier parameter lives in a frame slot.
    function clamped(uint256 x) public clamp(x) returns (uint256) {
        return t;
    }

    modifier remember(uint256 x) {
        uint256 local = x;
        x += 10;
        _;
        t = t * 1000 + local * 100 + x;
    }

    // Re-entering the same HIR modifier body must allocate a distinct `x`
    // and `local` activation for the inner application.
    function repeatedInner() internal remember(1) remember(2) {
        t = 0;
    }

    function repeated() public returns (uint256) {
        repeatedInner();
        return t;
    }
}

contract ModBase {
    uint256 internal t;

    modifier vguard() virtual {
        t += 1;
        _;
    }

    function guardedV() public vguard returns (uint256) {
        return t;
    }
}

// The inherited function must run the most-derived override of the modifier.
contract ModDerived is ModBase {
    modifier vguard() override {
        t += 100;
        _;
    }
}

contract CtorMod {
    uint256 public v;

    modifier init() {
        v = 42;
        _;
        v += 1;
    }

    constructor() init {
        v += 7;
    }
}
