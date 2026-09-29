//@ compile-flags: -Ogas -Zdump=evm-ir-runtime
//@ filecheck:

// The loop-invariant length of `RuntimeChild`'s initcode is computed outside the
// loop, yet its copy stays bounded, so the runtime drops constructor-only data.

contract InitOnlyChild {
    function f() external pure returns (uint256) {
        return 1;
    }
}

contract RuntimeChild {
    function g() external pure returns (uint256) {
        return 2;
    }
}

// CHECK-LABEL: hoisted_creation_size.sol:Factory (runtime) ===
// CHECK: @data RuntimeChild_initcode
// CHECK-NOT: @data InitOnlyChild_initcode
contract Factory {
    InitOnlyChild internal child = new InitOnlyChild();

    function make(uint256 count) external returns (address last) {
        for (uint256 i; i < count; ++i) {
            last = address(new RuntimeChild());
        }
    }
}
