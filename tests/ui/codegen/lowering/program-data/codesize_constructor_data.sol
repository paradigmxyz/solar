//@ codegen-matrix: standard optimized
//@[optimized] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[optimized] filecheck:
//@ run-call: Factory::value => 7
//@ run-call: Factory::sizeMatches => true

contract Child {
    function value() external pure returns (uint256) {
        return 7;
    }
}

// CHECK-LABEL: @module Factory_runtime
// CHECK-NOT: Child_creation_code
// CHECK: codesize
// CHECK-NOT: Child_creation_code
contract Factory {
    Child immutable child = new Child();

    function value() external view returns (uint256) {
        return child.value();
    }

    function sizeMatches() external view returns (bool) {
        uint256 size;
        assembly {
            size := codesize()
        }
        return size == address(this).code.length;
    }
}
