//@ compile-flags: --emit=bin-runtime -Zemit-lean-stages=../../target/ui-lean-stages
// A dynamic array returned from the heap and a fixed array codegen places below it: the entry's
// free memory pointer, the heap floor and the allocations are the words codegen placed, which the
// MIR stages take once codegen fixes them.

contract LeanStagesHeap {
    function pair(uint256 a) external pure returns (uint256[] memory r) {
        r = new uint256[](2);
        r[0] = a;
        r[1] = a + 1;
    }

    function name() external pure returns (string memory) {
        return "Tempo";
    }

    struct Point {
        uint256 x;
        uint256 y;
    }

    function triple(uint256 v) external pure returns (uint256) {
        uint256[3] memory a;
        a[1] = v;
        Point memory p = Point(v, v + 1);
        return a[1] + p.y;
    }
}
