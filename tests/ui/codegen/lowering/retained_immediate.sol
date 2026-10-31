//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas -Zdump=disasm-runtime
//@[gas] filecheck: --check-prefix=GAS
//@[size] compile-flags: -O size -Zdump=disasm-runtime
//@[size] filecheck: --check-prefix=SIZE
//@ run-call: evaluate 2, 3, 0 => 9
//@ run-call: evaluate 2, 3, 1 => 90
//@ run-call: evaluate 2, 3, 3 => 20250
//@ run-call: evaluate 0xffffffff00000001000000000000000000000000fffffffffffffffffffffffe, 0xffffffff00000001000000000000000000000000fffffffffffffffffffffffc, 1 => 9
//@ run-call: evaluate 0xffffffff00000001000000000000000000000001000000000000000000000004, 0xffffffff00000001000000000000000000000001000000000000000000000006, 0 => 42

contract RetainedImmediate {
    uint256 constant P = 0xffffffff00000001000000000000000000000000ffffffffffffffffffffffff;

    // GAS: PUSH16 0xfffffffeffffffffffffffffffffffff
    // GAS-NOT: PUSH16 0xfffffffeffffffffffffffffffffffff
    // SIZE: PUSH4 0xffffffff
    // SIZE-NOT: PUSH4 0xffffffff
    function evaluate(uint256 x, uint256 y, uint256 rounds) external pure returns (uint256) {
        x = mulmod(x, y, P);
        if (rounds == 0) return addmod(x, y, P);
        for (uint256 i; i < rounds; ++i) {
            uint256 v = mulmod(x, y, P);
            uint256 w = addmod(v, x, P);
            x = addmod(mulmod(w, y, P), v, P);
        }
        return x;
    }
}
