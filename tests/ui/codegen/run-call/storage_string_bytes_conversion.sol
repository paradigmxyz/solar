//@ run-call: readLength; constructor=["hello"] => 5
contract StorageStringBytesConversion {
    string private stored;

    constructor(string memory input) {
        stored = input;
    }

    function readLength() external view returns (uint256) {
        return bytes(stored).length;
    }
}
