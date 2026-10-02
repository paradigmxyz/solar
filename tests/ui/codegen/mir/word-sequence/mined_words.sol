//@ codegen-matrix: standard
//@ run-call: odd 0 => 1
//@ run-call: odd 9 => 19
//@ run-call: odd 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: odd 0x8000000000000000000000000000000000000000000000000000000000000000 => 1
//@ run-call: odd 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: doublePlus 9, 4 => 22
//@ run-call: doublePlus 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2 => 0
//@ run-call: shared 9, 4 => 18, 22
contract MinedWords {
    function odd(uint256 x) external pure returns (uint256 result) {
        assembly { result := or(shl(1, x), 1) }
    }
    function doublePlus(uint256 x, uint256 y) external pure returns (uint256 result) {
        assembly { result := add(shl(1, x), y) }
    }
    function shared(uint256 x, uint256 y) external pure returns (uint256 doubled, uint256 sum) {
        assembly { doubled := shl(1, x) sum := add(doubled, y) }
    }
}
