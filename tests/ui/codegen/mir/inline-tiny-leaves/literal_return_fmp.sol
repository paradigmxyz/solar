//@ codegen-matrix: standard
//@ run-call: retainedLiteral => 0x31, 0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000013100000000000000000000000000000000000000000000000000000000000000

contract LiteralReturnFmp {
    function retainedLiteral()
        external
        pure
        returns (bytes memory source, bytes memory encoded)
    {
        source = literal();
        encoded = abi.encode(source);
    }

    function literal() private pure returns (bytes memory) {
        return "1";
    }
}
