//@ compile-flags: -Zcodegen -O none -Zdump=mir
//@ filecheck: --check-prefix=BUILT

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

// BUILT-LABEL: @module PointerDerived
// BUILT-LABEL: fn @target(
// BUILT: ret 2
// BUILT-LABEL: fn @callQualified(
// BUILT: internal_call @__internal_dispatch_0, 1, [[BASE_TARGET:[0-9]+]]
// BUILT-LABEL: fn @__internal_dispatch_0(
// BUILT: eq arg0, [[BASE_TARGET]]
// BUILT: internal_call target{{[0-9]+}}, 1
// BUILT: eq arg0, [[DERIVED_TARGET:[0-9]+]]
// BUILT: internal_call target{{[0-9]+}}, 1
// BUILT-LABEL: fn @callVirtual(
// BUILT: internal_call @__internal_dispatch_0, 1, [[DERIVED_TARGET]]
// BUILT-LABEL: fn @callThroughVirtualPointer(
// BUILT: internal_call @__internal_dispatch_0, 1, [[DERIVED_TARGET]]
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
