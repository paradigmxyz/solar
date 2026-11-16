// A base that does not name a contract is dropped from the HIR while the AST
// keeps it. Pairing the two lists positionally moved `A(1)` onto the dropped
// base and left `A` with no arguments, adding a spurious 3415 on top of the
// resolution error. Only the resolution error is expected, like solc.
struct S {
    uint256 x;
}

contract A {
    constructor(uint256) {}
}

contract Unresolved is Undef, A(1) {} //~ ERROR: unresolved symbol `Undef`

contract NotAContract is S, A(1) {} //~ ERROR: expected contract, found struct
