//@ revisions: mir run
//@[mir] compile-flags: -Zmir-pipeline=specialize-function-pointers -Zdump=mir
//@[mir] filecheck:
//@[run] compile-flags: -Zmir-pipeline=specialize-function-pointers,lower-checks,lower-builtins,lower-arithmetic,lower-abi,lower-abi-encode,lower-dispatch,lower-frame-slots,lower-mapping-slots,lower-memory-objects,lower-slices,lower-alloc,lower-memory-zero,lower-integers,lower-evm-shaped
//@[run] normalize-stdout-test: "[0-9a-f]{40,}" -> "BYTECODE"
//@[run] run-call: callConstant 41 => 42
//@[run] run-call: choose true, 41 => 42
//@[run] run-call: choose false, 41 => 40
//@[run] run-call: throughCast 41 => 42
//@[run] run-call-fail: callConstant 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
// A call through a constant function pointer becomes a direct call, also through a function
// that only casts the pointer. A pointer chosen at run time keeps the dispatcher call.

contract ConstantTarget {
    // CHECK-LABEL: fn @callConstant(
    // CHECK: icall @increment, arg0
    function callConstant(uint256 value) public returns (uint256) {
        function(uint256) internal returns (uint256) fn = increment;
        return fn(value);
    }

    // CHECK-LABEL: fn @choose(
    // CHECK: [[FN:v[0-9]+]] = phi [bb1: 1], [bb2: 2]
    // CHECK: icall @internal_dispatcher{{.*}}, [[FN]], arg1
    function choose(bool add, uint256 value) public returns (uint256) {
        function(uint256) internal returns (uint256) fn = add ? increment : decrement;
        return fn(value);
    }

    // CHECK-LABEL: fn @throughCast(
    // CHECK: icall @incrementView, arg0
    function throughCast(uint256 value) public pure returns (uint256) {
        return castToPure(incrementView)(value);
    }

    function castToPure(
        function(uint256) internal view returns (uint256) fnIn
    ) internal pure returns (function(uint256) internal pure returns (uint256) fnOut) {
        assembly {
            fnOut := fnIn
        }
    }

    function incrementView(uint256 value) internal view returns (uint256) {
        if (block.number == type(uint256).max) return value;
        return value + 1;
    }

    function increment(uint256 value) internal pure returns (uint256) {
        return value + 1;
    }

    function decrement(uint256 value) internal pure returns (uint256) {
        return value - 1;
    }
}
