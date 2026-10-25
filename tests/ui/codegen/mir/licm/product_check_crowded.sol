//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Osize -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: crowded 3, 7, 1, 2, 3, 0 => 9
//@ run-call-fail: crowded 2, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1, 2, 3, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

contract ProductCheckCrowded {
    // A loop that already carries five words keeps its division: the bound would be a sixth.
    // CHECK-LABEL: @module ProductCheckCrowded_runtime
    // CHECK: [loop]:
    // CHECK: div
    function crowded(uint256 x, uint256 y, uint256 a, uint256 b, uint256 c, uint256 n)
        external
        pure
        returns (uint256)
    {
        uint256 s;
        for (uint256 i = 0; i < n; ++i) {
            x = x * y + 1;
            s += a ^ i;
            a = b + i;
            b = c ^ a;
            c = s + b;
        }
        return x + s + a + b + c;
    }
}
