//@ check-pass
//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

contract AbiDynamicReturn {
    function bytesLiteral() public pure returns (bytes memory) {
        return hex"010203";
    }

    function stringLiteral() public pure returns (string memory) {
        return "hello";
    }
}
