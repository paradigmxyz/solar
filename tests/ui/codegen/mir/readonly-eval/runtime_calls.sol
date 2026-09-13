//@ codegen-matrix: standard
//@ run-call: constantWord => 42
//@ run-call: dynamicWord 511 => 511
//@ run-call: afterOverwrite 511 => 511
//@ run-call: constantShort => 0x3100000000000000000000000000000000000000000000000000000000000001
//@ run-call: dynamicShort 0x31 => 0x3100000000000000000000000000000000000000000000000000000000000001
//@ run-call: dynamicShort 0x => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: dynamicShort 0x0000000000000000000000000000000000000000000000000000000000000000

// A constructor-free target also supports bounded symbolic runtime comparisons.
contract RuntimeCalls {
    function constantWord() external pure returns (uint256) {
        bytes memory data = new bytes(32);
        assembly { mstore(add(data, 32), 42) }
        return read(data);
    }

    function dynamicWord(uint256 value) external pure returns (uint256) {
        bytes memory data = new bytes(32);
        assembly { mstore(add(data, 32), value) }
        return read(data);
    }

    function afterOverwrite(uint256 value) external pure returns (uint256) {
        bytes memory data = new bytes(32);
        assembly { mstore(add(data, 32), 42) }
        overwrite(data, value);
        return read(data);
    }

    function constantShort() external pure returns (bytes32) {
        return short(bytes("1"));
    }

    function dynamicShort(bytes memory data) external pure returns (bytes32) {
        return short(data);
    }

    function read(bytes memory data) internal pure returns (uint256 result) {
        require(data.length == 32);
        assembly { result := mload(add(data, 32)) }
    }

    function short(bytes memory data) internal pure returns (bytes32 result) {
        require(data.length < 32);
        assembly {
            result := or(
                and(mload(add(data, 32)), shl(mul(sub(32, mload(data)), 8), not(0))),
                mload(data)
            )
        }
    }

    function overwrite(bytes memory data, uint256 value) internal pure {
        assembly { mstore(add(data, 32), value) }
    }
}
