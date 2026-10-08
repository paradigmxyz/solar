//@ codegen-matrix: standard
//@ run-call: declareStorage => 6, 0x68656c6c6f21
//@ run-call: declareMemory => 0x68656c6c6f

// `bytes(s)` on a storage string is a storage reference to the same slot.

contract TupleDeclBytesConversion {
    string text = "hello";

    function declareStorage() external returns (uint256, bytes memory) {
        (bytes storage data) = bytes(text);
        data.push(0x21);
        return (data.length, bytes(text));
    }

    function declareMemory() external view returns (bytes memory) {
        (bytes memory data) = bytes(text);
        return data;
    }
}
