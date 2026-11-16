//@ codegen-matrix: standard
//@ run-call: storeLoad 1, 258 => 256
//@ run-call: storeLoad 255, 0 => 0
//@ run-call: storeLoad 0, 0 => 0
//@ run-call: storeLoad 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
contract StackMemoryWindow {
    function storeLoad(uint256 x, uint256 y) external pure returns (uint256 result) {
        assembly {
            let p := add(mload(0x40), and(x, 31))
            mstore(0x40, add(p, 0x40))
            let value := xor(x, y)
            mstore(p, value)
            mstore8(add(p, 31), byte(0, value))
            result := mload(p)
        }
    }
}
