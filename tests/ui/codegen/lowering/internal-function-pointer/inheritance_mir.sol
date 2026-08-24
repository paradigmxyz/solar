//@ compile-flags: -O none -Zdump=mir
//@ filecheck:

// ported-from: test/libsolidity/semanticTests/inheritance/inherited_function_through_dispatch.sol
// ported-from: test/libsolidity/semanticTests/virtualFunctions/internal_virtual_function_calls_through_dispatch.sol

contract PointerBase {
    function target() internal virtual returns (uint256) {
        return 1;
    }

    function callThroughVirtualPointer() internal returns (uint256) {
        function() internal returns (uint256) fn = target;
        return fn();
    }
}

// CHECK-LABEL: @module PointerDerived
// CHECK-LABEL: fn @target.0(
// CHECK: ret 2
// CHECK-LABEL: fn @callQualified(
// CHECK: internal_call @internal_dispatcher{{.*}}, 1, [[BASE_TARGET:[0-9]+]]
// CHECK-LABEL: fn @callVirtual(
// CHECK: internal_call @callThroughVirtualPointer, 1
// CHECK-LABEL: fn @callThroughVirtualPointer(
// CHECK: internal_call @internal_dispatcher{{.*}}, 1, [[DERIVED_TARGET:[0-9]+]]
// CHECK-LABEL: fn @internal_dispatcher{{.*}}(
// CHECK: eq arg0, [[BASE_TARGET]]
// CHECK: internal_call @target.{{[0-9]+}}, 1
// CHECK: eq arg0, [[DERIVED_TARGET]]
// CHECK: internal_call @target.{{[0-9]+}}, 1
contract PointerDerived is PointerBase {
    function target() internal pure override returns (uint256) {
        return 2;
    }

    function callQualified() public returns (uint256) {
        function() internal returns (uint256) fn = PointerBase.target;
        return fn();
    }

    function callVirtual() public returns (uint256) {
        return callThroughVirtualPointer();
    }
}
