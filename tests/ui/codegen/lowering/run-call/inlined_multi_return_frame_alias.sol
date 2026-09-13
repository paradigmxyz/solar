//@ codegen-matrix: standard
//@ run-call: Test::run => 0x1234
//@ run-call: TransitiveTupleMemory::run 7 => 15, 0

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

contract TransitiveTupleMemory {
    function run(uint256 input) external pure returns (uint256 sum, uint256 word) {
        (uint256 first, uint256 second) = pair(input);
        word = observe();
        sum = first + second;
    }

    function pair(uint256 input) internal pure returns (uint256, uint256) {
        return (input, input + 1);
    }

    function observe() internal pure returns (uint256 word) {
        assembly { mstore(0x40, 0x80) }
    }
}
