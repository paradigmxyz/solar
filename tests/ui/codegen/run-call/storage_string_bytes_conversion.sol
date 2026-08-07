//@ run-call: readLength; constructor=["hello"] => 5
//@ run-call: readBytes; constructor=["hello"] => 0x68656c6c6f
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
}
