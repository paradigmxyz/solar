library MessageHashUtils {
    function toEthSignedMessageHash(bytes32 messageHash) internal pure returns (bytes32) {
        return messageHash;
    }

    function toEthSignedMessageHash(bytes memory message) internal pure returns (bytes32) {
        return keccak256(message);
    }
}

contract C {
    function hash(bytes32 messageHash) internal pure returns (bytes32) {
        return messageHash;
    }

    function hash(bytes memory message) internal pure returns (bytes32) {
        return keccak256(message);
    }

    // A literal longer than 32 bytes only converts to `bytes memory`.
    function longLiteral() public pure {
        MessageHashUtils.toEthSignedMessageHash("I verify I am human using Worldchain");
        hash("I verify I am human using Worldchain");
    }

    // A 32-byte literal converts to both `bytes32` and `bytes memory`.
    function shortLiteral() public pure {
        MessageHashUtils.toEthSignedMessageHash("abcdefghijklmnopqrstuvwxyz012345"); //~ ERROR: member `toEthSignedMessageHash` not unique
        hash("abcdefghijklmnopqrstuvwxyz012345"); //~ ERROR: no unique declarations found
    }
}
