//@compile-flags: -Zcodegen --emit=mir

contract StorageBytesFromCalldata {
    string text;
    bytes blob;

    function setText(string calldata value) external {
        text = value;
    }

    function setBlob(bytes calldata value) external {
        blob = value;
    }

    function getText() external view returns (string memory) {
        return text;
    }

    function getBlob() external view returns (bytes memory) {
        return blob;
    }
}
