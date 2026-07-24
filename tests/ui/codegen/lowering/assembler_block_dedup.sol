//@ check-pass
//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime --pretty-json
contract AssemblerBlockDedup {
    function a() public pure returns (uint256) {
        return 1;
    }

    function b() public pure returns (uint256) {
        return 1;
    }

    function c(bool fail) public pure returns (uint256) {
        if (fail) revert();
        return 2;
    }

    function d(bool fail) public pure returns (uint256) {
        if (fail) revert();
        return 2;
    }
}
