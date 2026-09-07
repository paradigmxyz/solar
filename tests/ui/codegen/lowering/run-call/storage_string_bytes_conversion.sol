//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: readLength; constructor=["hello"] => 5
//@ run-call: readBytes; constructor=["hello"] => 0x68656c6c6f
//@ run-call: readBytes; constructor=["abcdefghijklmnopqrstuvwxyz0123456789"] => 0x6162636465666768696a6b6c6d6e6f707172737475767778797a30313233343536373839
//@ run-call: pushThroughConversion; constructor=["hello"] => 0x68656c6c6f21
//@ run-call: pushAssignThroughConversion; constructor=["hello"] => 0x68656c6c6f21
//@ run-call: popThroughConversion; constructor=["hello"] => 0x68656c6c
//@ run-call: transitionThroughConversion; constructor=["hello"] => 0x68656c6c6f4141414141414141414141414141414141414141414141414141, 0x68656c6c6f41414141414141414141414141414141414141414141414141413e
contract StorageStringBytesConversion {
    string private stored;

    constructor(string memory input) {
        stored = input;
    }

    function readLength() external view returns (uint256) {
        return bytes(stored).length;
    }

    function readBytes() external view returns (bytes memory) {
        return bytes(stored);
    }

    function pushThroughConversion() external returns (bytes memory) {
        (bytes(string(bytes(stored)))).push(0x21);
        return bytes(stored);
    }

    function pushAssignThroughConversion() external returns (bytes memory) {
        bytes(stored).push() = 0x21;
        return bytes(stored);
    }

    function popThroughConversion() external returns (bytes memory) {
        bytes(stored).pop();
        return bytes(stored);
    }

    // Growing and shrinking through the conversion updates the string's own
    // slot in place, so both short/long transitions run on it: the pushes cross
    // 32 bytes and the pop moves the data back into the header.
    function transitionThroughConversion() external returns (bytes memory, bytes32 header) {
        while (bytes(stored).length < 32) {
            bytes(stored).push(0x41);
        }
        bytes(stored).pop();
        assembly {
            header := sload(stored.slot)
        }
        return (bytes(stored), header);
    }
}
