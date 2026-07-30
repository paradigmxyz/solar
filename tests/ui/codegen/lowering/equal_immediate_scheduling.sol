//@ revisions: none gas size
//@[none] compile-flags: -Zcodegen -O none -Zdump=evm-ir-runtime
//@[none] filecheck: --check-prefix=NONE
//@[gas] compile-flags: -Zcodegen -O gas -Zdump=evm-ir-runtime
//@[gas] filecheck: --check-prefix=GAS
//@[size] compile-flags: -Zcodegen -O size -Zdump=evm-ir-runtime
//@[size] filecheck: --check-prefix=SIZE

contract EqualImmediateScheduling {
    // NONE-COUNT-2: push 0x123456789abcde
    // GAS-COUNT-2: push 0x123456789abcde
    // SIZE: push 0x123456789abcde
    // SIZE-NOT: push 0x123456789abcde
    // SIZE: dup1
    function combine(uint256 modulus) public pure returns (uint256) {
        return addmod(0x123456789abcde, 0x123456789abcde, modulus);
    }
}
