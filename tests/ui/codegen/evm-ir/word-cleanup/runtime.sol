//@ codegen-matrix: standard
//@ run-call: callerRoundTrip => true
//@ run-call: producer => true
//@ run-call: addressConsumer 0xffffffffffffffffffffffff0000000000000000000000000000000000000004 => 0
//@ run-call: shared 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffff
//@ run-call: byteConsumer 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe => 5
//@ run-call: dirtyRead => 0x10000000000000000000000000000000000000002
//@ run-call: dirtyReturn => 0x0000000000000000000000000000000000000002
//@ run-call: callConsumer 0xffffffffffffffffffffffff0000000000000000000000000000000000000004 => 42
//@ run-call: dependentMasks 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe => 254
//@ run-call: nestedMasks 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 255
contract Cleanup {
    function dependentMasks(uint256 x, uint256 y) external pure returns (uint256 result) {
        assembly { result := and(and(x, 255), and(y, 255)) }
    }

    function nestedMasks(uint256 x) external pure returns (uint256 result) {
        assembly { result := and(and(x, 255), 255) }
    }

    function callerRoundTrip() external view returns (bool) {
        address sender = msg.sender;
        uint256 raw;
        assembly { raw := sender }
        return address(uint160(raw)) == msg.sender;
    }

    function producer() external view returns (bool result) {
        assembly {
            result := eq(and(caller(), 0xffffffffffffffffffffffffffffffffffffffff), caller())
        }
    }

    function addressConsumer(uint256 raw) external view returns (uint256) {
        return address(uint160(raw)).code.length;
    }

    function shared(uint256 raw) external view returns (uint256 result) {
        assembly {
            let cleaned := and(raw, 0xffffffffffffffffffffffffffffffffffffffff)
            mstore(0, extcodesize(cleaned))
            mstore(32, cleaned)
            result := mload(32)
        }
    }

    function byteConsumer(uint256 raw) external pure returns (uint256 result) {
        assembly {
            mstore(0, 0)
            mstore8(31, add(and(raw, 65535), 7))
            result := mload(0)
        }
    }

    function dirtyRead() external pure returns (uint256 raw) {
        address value;
        assembly {
            value := 0x10000000000000000000000000000000000000002
            raw := value
        }
    }

    function dirtyReturn() external pure returns (address value) {
        assembly { value := 0x10000000000000000000000000000000000000002 }
    }

    function callConsumer(uint256 raw) external view returns (uint256 result) {
        (bool success, bytes memory output) = address(uint160(raw)).staticcall(abi.encode(uint256(42)));
        require(success);
        return abi.decode(output, (uint256));
    }
}
