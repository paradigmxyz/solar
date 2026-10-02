//@ codegen-matrix: standard
//@ run-call: once 0x00000000000000000000000000000000000000000000000000000000000000ff => 0x0000000000000000000000000000000000000000000000000000000000000100
//@ run-call: twice 0x0000000000000000000000000000000000000000000000000000000000000000 => 0x0000000000000000000000000000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: once 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x0000000000000000000000000000000000000000000000000000000000000000
contract SharedMemoryWrapper {
    function once(bytes memory input) external pure returns (bytes memory) {
        return wrap(input);
    }

    // Each nested call allocates a fresh copy. The post-call store must change
    // only that copy, preserving both the other result and the original input.
    function twice(bytes memory input) external pure returns (bytes memory, bytes memory, bytes memory) {
        bytes memory first = wrap(input);
        bytes memory second = wrap(input);
        return (first, second, input);
    }

    function wrap(bytes memory input) internal pure returns (bytes memory result) {
        result = copy(input);
        assembly {
            let p := add(result, 32)
            mstore(p, add(mload(p), 1))
        }
    }

    function copy(bytes memory input) internal pure returns (bytes memory result) {
        result = new bytes(input.length);
        for (uint256 i; i < input.length; ++i) {
            result[i] = input[i];
        }
    }
}
