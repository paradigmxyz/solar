//@ codegen-matrix: standard
//@ run-call: check 0 => 14, 0
//@ run-call: check 255 => 14, 255
//@ run-call: check 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0e, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
contract Test {
    function temporary(uint256 p) internal pure returns (uint256 result) {
        assembly {
            let saved := mload(p)
            mstore8(add(p, 31), 7)
            result := mload(p)
            mstore(p, saved)
        }
    }
    function check(uint256 value) external pure returns (uint256 result, uint256 restored) {
        uint256 p;
        assembly {
            p := mload(0x40)
            mstore(0x40, add(p, 32))
            mstore(p, value)
        }
        unchecked { result = temporary(p) + temporary(p); }
        assembly { restored := mload(p) }
    }
}
