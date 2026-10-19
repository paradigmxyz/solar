//@ codegen-matrix: standard
//@ run-call: overwritten 0x0000000000000000000000000000000000000000000000000000000000000000 => 0x00
//@ run-call: overwritten 0x4200000000000000000000000000000000000000000000000000000000000000 => 0x42

contract LiteralAlias {
    // The payload is reachable through the heap frontier without a direct use
    // of `result`. A literal recognizer must account for the intervening write.
    function overwritten(bytes32 word) external pure returns (bytes memory) {
        bytes memory result = literal();
        assembly { mstore(sub(mload(0x40), 32), word) }
        return result;
    }

    function literal() private pure returns (bytes memory) {
        return "1";
    }
}
