//@ run-call: callLiteral() => 0x312e302e31000000000000000000000000000000000000000000000000000000

contract ExternalStringLiteralFixedBytes {
    function take(bytes32 value) external pure returns (bytes32) {
        return value;
    }

    function callLiteral() external view returns (bytes32) {
        return this.take("1.0.1");
    }
}
