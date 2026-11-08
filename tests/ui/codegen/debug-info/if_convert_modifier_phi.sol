//@ codegen-matrix: standard
//@ compile-flags: -Zdebugify=check
//@ normalize-stdout-test: "(?s).+" -> ""
//@ normalize-stderr-test: "debugify: [^\n]*\n" -> ""
//@ run-call: maybe(bool) true => 0
//@ run-call: maybe(bool) false => 7
// Hoisted operations must not inherit the source location of the unexecuted modifier arm.
contract ModifierPhi {
    modifier skipIf(bool c) {
        if (c) return;
        _;
    }

    function maybe(bool c) public pure skipIf(c) returns (uint256 out) {
        out = 7;
    }
}
