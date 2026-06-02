//@ignore-host: windows
//@compile-flags: --emit=mir

contract AbiDynamicReturn {
    function bytesLiteral() public pure returns (bytes memory) {
        return hex"010203";
    }

    function stringLiteral() public pure returns (string memory) {
        return "hello";
    }
}
