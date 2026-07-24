//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime --pretty-json
//@ filecheck: --check-prefix=DEDUP
contract AssemblerBlockDedup {
    // DEDUP: push 0xdbe671f
    // DEDUP: push [[ONE:bb[0-9]+]]
    function a() public pure returns (uint256) {
        return 1;
    }

    // DEDUP: push 0x4df7e3d0
    // DEDUP: push [[ONE]]
    function b() public pure returns (uint256) {
        return 1;
    }

    // DEDUP: push 0x5ce8bda8
    // DEDUP: push [[TWO:bb[0-9]+]]
    function c(bool fail) public pure returns (uint256) {
        if (fail) revert();
        return 2;
    }

    // DEDUP: push 0xfeb97429
    // DEDUP: push [[TWO]]
    // DEDUP: [[ONE]]:
    // DEDUP: push 1
    // DEDUP: jump [[RETURN:bb[0-9]+]]
    // DEDUP: [[RETURN]]:
    // DEDUP: return
    // DEDUP: [[TWO]]:
    // DEDUP: push [[REVERT:bb[0-9]+]]
    // DEDUP: jumpi
    // DEDUP: push 2
    // DEDUP: jump [[RETURN]]
    function d(bool fail) public pure returns (uint256) {
        if (fail) revert();
        return 2;
    }
}
