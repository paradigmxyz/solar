//@ codegen-matrix: standard
//@ run-call: Test::run => 0x1234

// An inlined multi-return callee stages its tail values in the caller's
// compiler-owned frame. The projection reads the published base through
// scratch slot 0x20; retain the base's SSA identity so memory DSE cannot
// mistake the frame store for dead memory.

contract Test {
    function run() external pure returns (uint256) {
        return outer();
    }

    function outer() internal pure returns (uint256 result) {
        (, result) = pair();
    }

    function pair() internal pure returns (uint256 first, uint256 second) {
        first = 0xabcd;
        second = 0x1234;
    }
}
