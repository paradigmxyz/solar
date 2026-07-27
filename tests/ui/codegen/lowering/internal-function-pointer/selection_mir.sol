//@ revisions: built opt
//@[built] compile-flags: -Zcodegen -Zdump=mir
//@[built] filecheck: --check-prefix=BUILT
//@[opt] compile-flags: -Zcodegen -Ogas -Zdump=mir-evm-shaped
//@[opt] filecheck: --check-prefix=OPT

// OPT: @phase evm-shaped
contract FunctionPointerSelection {
    // BUILT-LABEL: fn @choose(
    // BUILT: mstore 0, [[INCREMENT:[0-9]+]]
    // BUILT: mstore 0, [[DECREMENT:[0-9]+]]
    // BUILT: internal_call @__internal_dispatch_0, 1, {{v[0-9]+}}, arg1
    // BUILT-LABEL: fn @__internal_dispatch_0(
    // BUILT: eq arg0, [[INCREMENT]]
    // BUILT: internal_call @increment, 1, arg1
    // BUILT: eq arg0, [[DECREMENT]]
    // BUILT: internal_call @decrement, 1, arg1
    // BUILT: eq arg0, [[INCREMENT_VIEW:[0-9]+]]
    // BUILT: internal_call @incrementView, 1, arg1
    // BUILT: mstore 4, 81
    // BUILT: revert 0, 36
    // OPT-LABEL: fn @choose(
    // OPT: internal_call @__internal_dispatch_0, 1, {{v[0-9]+}}, arg1
    // OPT-LABEL: fn @__internal_dispatch_0(
    // OPT: eq arg0, {{[0-9]+}}
    // OPT: eq arg0, {{[0-9]+}}
    // OPT: eq arg0, {{[0-9]+}}
    // OPT: mstore 4, 81
    // OPT: tail_call @[[ARITH:__revert_stub[0-9]+]]
    function choose(bool add, uint256 value) public returns (uint256) {
        function(uint256) internal returns (uint256) fn = add ? increment : decrement;
        return fn(value);
    }

    function increment(uint256 value) internal pure returns (uint256) {
        return value + 1;
    }

    function decrement(uint256 value) internal pure returns (uint256) {
        return value - 1;
    }

    // BUILT-LABEL: fn @callConstant(
    // BUILT: internal_call @__internal_dispatch_0, 1, [[INCREMENT]], arg0
    // OPT-LABEL: fn @callConstant(
    // OPT-NOT: internal_call @__internal_dispatch
    // OPT: add arg0, 1
    // OPT: returndata 128, 32
    function callConstant(uint256 value) public returns (uint256) {
        function(uint256) internal returns (uint256) fn = increment;
        return fn(value);
    }

    // BUILT-LABEL: fn @throughCast(
    // BUILT: [[CASTED:v[0-9]+]] = internal_call @castViewToPure, 1, [[INCREMENT_VIEW]]
    // BUILT: internal_call @__internal_dispatch_0, 1, [[CASTED]], arg0
    // BUILT-LABEL: fn @castViewToPure(
    // BUILT: mstore {{v[0-9]+}}, arg0
    // BUILT: ret {{v[0-9]+}}
    // OPT-LABEL: fn @throughCast(
    // OPT-NOT: internal_call @__internal_dispatch
    // OPT: number
    // OPT: add arg0, 1
    function throughCast(uint256 value) public pure returns (uint256) {
        return castViewToPure(incrementView)(value);
    }

    function castViewToPure(
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

    // OPT: fn @[[ARITH]](
    // OPT: mstore 4, 17
}
