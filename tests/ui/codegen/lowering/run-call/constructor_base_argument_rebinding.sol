//@ codegen-matrix: standard
//@ run-call: ScalarRebind::a; constructor=[3] => 7
//@ run-call: ScalarRebind::b; constructor=[3] => 7
//@ run-call: StorageRebind::first => 0
//@ run-call: StorageRebind::second => 2

struct S {
    uint256 x;
}

abstract contract ScalarBase {
    uint256 public a;

    constructor(uint256 x) {
        a = x;
    }
}

// Every base argument list is evaluated before the first base body runs, so
// `y` is 7 by the time this body reads it, whatever the derived contract
// passed for it.
abstract contract ScalarMiddle is ScalarBase {
    uint256 public b;

    constructor(uint256 y) ScalarBase(y = 7) {
        b = y;
    }
}

contract ScalarRebind is ScalarMiddle {
    constructor(uint256 z) ScalarMiddle(z) {}
}

abstract contract StorageBase {
    constructor(S storage s) {
        s.x = 1;
    }
}

// The argument list rebinds the storage reference `s`, so this body writes
// through `t` and the slot `s` started at stays untouched.
abstract contract StorageMiddle is StorageBase {
    constructor(S storage s, S storage t) StorageBase(s = t) {
        s.x = 2;
    }
}

contract StorageRebind is StorageMiddle {
    S internal p;
    S internal q;

    constructor() StorageMiddle(p, q) {}

    function first() external view returns (uint256) {
        return p.x;
    }

    function second() external view returns (uint256) {
        return q.x;
    }
}
