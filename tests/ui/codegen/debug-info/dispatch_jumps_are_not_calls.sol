//@ revisions: none gas size
//@ compile-flags: --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@[none] compile-flags: -Onone
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The dispatcher jumps into an external function, which ends in a halt: no `o` follows, and solc
// never marks a `JUMPI` as `i`. Neither the jumps into the functions nor their `RETURN`s are
// marked.
// CHECK-LABEL: "srcmap-runtime"
// CHECK-NOT: :i
// CHECK-NOT: :o
contract C {
    uint256 x;

    function set(uint256 v) external {
        x = v;
    }

    function get() external view returns (uint256) {
        return x;
    }

    function twice() external view returns (uint256) {
        return x * 2;
    }
}
