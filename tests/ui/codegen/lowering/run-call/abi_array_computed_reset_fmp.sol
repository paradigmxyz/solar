//@ codegen-matrix: standard
//@ run-call: encode 7 => 2530
//@ run-call: encode 23 => 2546
//@ run-call: encodeTwice 7 => 5061
//@ run-call: readAfterRestore 7 => 22

contract AbiArrayComputedResetFmp {
    function encode(uint256 value) public pure returns (uint256) {
        uint256 saved = value + 1;
        uint256[][] memory values;
        assembly {
            values := add(value, 0x10000)
            mstore(values, 1)
            mstore(add(values, 32), add(values, 64))
            mstore(add(values, 64), 20)
            mstore(add(values, 96), 3)
            mstore(add(values, 128), 5)
            let base := mload(0x40)
            mstore(add(base, sub(0x40, base)), 0x80)
        }
        bytes memory encoded = abi.encode(values, values);
        uint256 sum;
        assembly {
            let end := add(add(encoded, 32), mload(encoded))
            for { let p := add(encoded, 32) } lt(p, end) { p := add(p, 32) } {
                sum := add(sum, mload(p))
            }
        }
        return saved + encoded.length + sum;
    }
    function encodeTwice(uint256 value) external pure returns (uint256) {
        return encode(value) + encode(value + 1);
    }

    function readAfterRestore(uint256 value) external pure returns (uint256) {
        uint256[] memory values;
        assembly {
            let saved := mload(0x40)
            mstore(0x40, 0x80)
            values := mload(add(value, sub(0x40, value)))
            mstore(values, 1)
            mstore(add(values, 32), value)
            mstore(0x40, saved)
        }
        uint256 doubled = doubleValue(value);
        return readLow(values) + doubled;
    }

    function readLow(uint256[] memory values) public pure returns (uint256) {
        return values[0];
    }

    function doubleValue(uint256 value) public pure returns (uint256) {
        return value * 2 + 1;
    }
}
