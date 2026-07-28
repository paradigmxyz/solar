//@ compile-flags: -Zcodegen -O none -Zdump=mir
//@ filecheck:

contract FunctionPointerSelection {
    // CHECK-LABEL: fn @choose(
    // CHECK: mstore 0, [[INCREMENT:[0-9]+]]
    // CHECK: mstore 0, [[DECREMENT:[0-9]+]]
    // CHECK: internal_call @__internal_dispatch_0, 1, {{v[0-9]+}}, arg1
    // CHECK-LABEL: fn @__internal_dispatch_0(
    // CHECK: eq arg0, [[INCREMENT]]
    // CHECK: internal_call @increment, 1, arg1
    // CHECK: eq arg0, [[DECREMENT]]
    // CHECK: internal_call @decrement, 1, arg1
    // CHECK: eq arg0, [[INCREMENT_VIEW:[0-9]+]]
    // CHECK: internal_call @incrementView, 1, arg1
    // CHECK: mstore 4, 81
    // CHECK: revert 0, 36
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

    // CHECK-LABEL: fn @callConstant(
    // CHECK: internal_call @__internal_dispatch_0, 1, [[INCREMENT]], arg0
    function callConstant(uint256 value) public returns (uint256) {
        function(uint256) internal returns (uint256) fn = increment;
        return fn(value);
    }

    // CHECK-LABEL: fn @throughCast(
    // CHECK: [[CASTED:v[0-9]+]] = internal_call @castViewToPure, 1, [[INCREMENT_VIEW]]
    // CHECK: internal_call @__internal_dispatch_0, 1, [[CASTED]], arg0
    // CHECK-LABEL: fn @castViewToPure(
    // CHECK: mstore {{v[0-9]+}}, arg0
    // CHECK: ret {{v[0-9]+}}
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
}
