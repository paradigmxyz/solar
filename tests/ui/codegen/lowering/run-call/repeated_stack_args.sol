//@ codegen-matrix: standard legacy-stack
//@[legacy-stack] compile-flags: -O size -Zlegacy-stack-lowering -Zdump=evm-ir-runtime
//@[legacy-stack] filecheck:
//@[size] compile-flags: -Zdump=evm-ir-runtime
//@[size] filecheck: --check-prefix=STACK
//@ run-call: run 2 => 63
//@ run-call: run 0 => 27

contract RepeatedStackArgs {
    uint256 private state;

    // CHECK-LABEL: @module RepeatedStackArgs_runtime
    // CHECK: add
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: push [[RETURN:bb[0-9]+]]
    // CHECK-NEXT: swap 2
    // CHECK-NEXT: jump
    // STACK-LABEL: @module RepeatedStackArgs_runtime
    // STACK: add
    // STACK-NEXT: dup 1
    // STACK-NEXT: push [[RETURN:bb[0-9]+]]
    // STACK-NEXT: jump
    function run(uint256 x) external returns (uint256 result) {
        unchecked {
            uint256 a = x + 1;
            result = mix(a, a);
            result += mix(x, 1);
            result += mix(x, 2);
            result += mix(x, 3);
        }
    }

    function mix(uint256 a, uint256 b) internal returns (uint256) {
        unchecked {
            uint256 c = a + b;
            uint256 d = a * 3;
            uint256 e = d ^ b;
            state ^= c;
            c += state;
            d ^= c;
            e += d;
            return e;
        }
    }
}
