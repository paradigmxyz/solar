//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=evm-ir-runtime
//@[none] filecheck: --check-prefix=NONE
//@[gas] compile-flags: -O gas -Zdump=evm-ir-runtime,disasm-runtime
//@[gas] filecheck: --check-prefix=GAS
//@[size] compile-flags: -O size -Zdump=evm-ir-runtime,disasm-runtime
//@[size] filecheck: --check-prefix=SIZE

contract EqualImmediateScheduling {
    // NONE-COUNT-2: push 0x123456789abcde
    // GAS: push 0x123456789abcde
    // GAS-NOT: push 0x123456789abcde
    // GAS: dup 1
    // GAS: CALLVALUE
    // GAS-NEXT: PUSH{{[0-9]+}} {{0x[0-9a-f]+}} ; [[GAS_REJECT:bb[0-9]+]]
    // GAS-NEXT: JUMPI
    // GAS: {{^; }}[[GAS_REJECT]]{{$}}
    // GAS-NEXT: JUMPDEST
    // GAS-NEXT: PUSH0
    // GAS-NEXT: PUSH0
    // GAS-NEXT: REVERT
    // SIZE: push 0x123456789abcde
    // SIZE-NOT: push 0x123456789abcde
    // SIZE: dup 1
    // SIZE: CALLVALUE
    // SIZE-NEXT: PUSH{{[0-9]+}} {{0x[0-9a-f]+}} ; [[SIZE_REJECT:bb[0-9]+]]
    // SIZE-NEXT: JUMPI
    // SIZE: {{^; }}[[SIZE_REJECT]]{{$}}
    // SIZE-NEXT: JUMPDEST
    // SIZE-NEXT: PUSH0
    // SIZE-NEXT: PUSH0
    // SIZE-NEXT: REVERT
    function combine(uint256 modulus) public pure returns (uint256) {
        return addmod(0x123456789abcde, 0x123456789abcde, modulus);
    }
}
