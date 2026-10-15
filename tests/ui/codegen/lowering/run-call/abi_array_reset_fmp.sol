//@ codegen-matrix: standard
//@ run-call: encode 7 => 2530
//@ run-call: encode 23 => 2546
//@ run-call: encodeTwice 7 => 5061
//@ run-call: callVoidAfterReset 7 => 14

contract AbiArrayResetFmp {
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
            mstore(0x40, 0x80)
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

    function callVoidAfterReset(uint256 value) external pure returns (uint256) {
        uint256 saved = value * 2;
        assembly { mstore(0x40, 0x80) }
        writeAtFmp(value);
        return saved;
    }

    function writeAtFmp(uint256 value) public pure {
        assembly { mstore(mload(0x40), value) }
    }
}
