//@ codegen-matrix: standard
//@ run-call: pair 3, 7 => 7, 3
//@ run-call: pair 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0
//@ run-call: triple 1, 2, 3 => 3, 1, 2
contract Test {
    function pair(uint256 a, uint256 b) external pure returns (uint256 x, uint256 y) {
        assembly {
            function swap(a_, b_) -> x_, y_ { x_ := b_ y_ := a_ }
            x, y := swap(a, b)
        }
    }
    function triple(uint256 a, uint256 b, uint256 c) external pure returns (uint256 x, uint256 y, uint256 z) {
        assembly {
            function rotate(a_, b_, c_) -> x_, y_, z_ { x_ := c_ y_ := a_ z_ := b_ }
            x, y, z := rotate(a, b, c)
        }
    }
}
