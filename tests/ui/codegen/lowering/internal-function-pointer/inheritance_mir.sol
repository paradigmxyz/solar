//@ revisions: built opt
//@[built] compile-flags: -Zcodegen -Zdump=mir
//@[built] filecheck: --check-prefix=BUILT
//@[opt] compile-flags: -Zcodegen -Ogas -Zdump=mir-evm-shaped
//@[opt] filecheck: --check-prefix=OPT

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
// BUILT: internal_call @target._1, 1
// BUILT: eq arg0, [[DERIVED_TARGET:[0-9]+]]
// BUILT: internal_call @target, 1
// BUILT-LABEL: fn @callVirtual(
// BUILT: internal_call @__internal_dispatch_0, 1, [[DERIVED_TARGET]]
// BUILT-LABEL: fn @callThroughVirtualPointer(
// BUILT: internal_call @__internal_dispatch_0, 1, [[DERIVED_TARGET]]
// OPT-LABEL: @module PointerDerived
// OPT: @phase evm-shaped
// OPT-LABEL: fn @callQualified(
// OPT: mstore 128, 1
// OPT-LABEL: fn @callVirtual(
// OPT: mstore 128, 2
// OPT-NOT: fn @__internal_dispatch
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
