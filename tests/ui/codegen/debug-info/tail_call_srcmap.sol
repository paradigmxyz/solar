//@ compile-flags: -Onone --emit=srcmap-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// `run` calls `outer` (`i`), `outer` tail calls `inner`, and `inner` returns (`o`). The tail call
// opens a frame and closes one, so its jump has no marker and the markers balance.
// CHECK-LABEL: "srcmap-runtime"
// CHECK-NOT: :i
// CHECK: :i
// CHECK-NOT: :i
// CHECK: :o
// CHECK-NOT: :o
contract C {
    function inner(uint256 x) internal pure returns (uint256) {
        return x * 3;
    }

    function outer(uint256 x) internal pure returns (uint256) {
        return inner(x + 1);
    }

    function run(uint256 a) external pure returns (uint256) {
        return outer(a);
    }
}
