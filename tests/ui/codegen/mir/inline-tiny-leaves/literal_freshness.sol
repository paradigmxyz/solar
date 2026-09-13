//@ codegen-matrix: standard
//@ run-call: encoded 7 => 0x123456780000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000000700000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000132000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
//@ run-call: freshInLoop 0x58 => true
//@ run-call: freshInLoop 0x00 => true
//@ run-call: emptyLiteral => 0

contract LiteralFreshness {
    function freshInLoop(bytes1 replacement) external pure returns (bool) {
        bytes memory previous;
        for (uint256 i; i < 2; ++i) {
            bytes memory current = literal();
            if (current.length != 1 || current[0] != 0x31) return false;
            if (i != 0 && previous[0] != replacement) return false;
            previous = current;
            current[0] = replacement;
        }
        return true;
    }

    function literal() private pure returns (bytes memory) {
        return "1";
    }

    function emptyLiteral() external pure returns (uint256) {
        return empty().length;
    }

    function empty() private pure returns (bytes memory) {
        return "";
    }
    function encoded(uint256 value) external pure returns (bytes memory) {
        return abi.encodeWithSelector(0x12345678, encodedWord(), value, bytes(""));
    }

    function encodedWord() private pure returns (bytes memory) {
        return "2";
    }
}
