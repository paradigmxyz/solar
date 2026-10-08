//@ run-call: LongStringLiteralOverloads::libraryCall => 0x955b4bf896fdb2ccd6955cffd8ae3ef2662c17959cb712671a898b312d603779
//@ run-call: LongStringLiteralOverloads::internalCall => 0x955b4bf896fdb2ccd6955cffd8ae3ef2662c17959cb712671a898b312d603779

// A string literal longer than 32 bytes does not convert to `bytes32`, so these calls select the
// `bytes memory` overload.
library MessageHashUtils {
    function toEthSignedMessageHash(bytes32 messageHash) internal pure returns (bytes32) {
        return messageHash;
    }

    function toEthSignedMessageHash(bytes memory message) internal pure returns (bytes32) {
        return keccak256(message);
    }
}

contract LongStringLiteralOverloads {
    function hash(bytes32 messageHash) internal pure returns (bytes32) {
        return messageHash;
    }

    function hash(bytes memory message) internal pure returns (bytes32) {
        return keccak256(message);
    }

    function libraryCall() external pure returns (bytes32) {
        return MessageHashUtils.toEthSignedMessageHash("I verify I am human using Worldchain");
    }

    function internalCall() external pure returns (bytes32) {
        return hash("I verify I am human using Worldchain");
    }
}
