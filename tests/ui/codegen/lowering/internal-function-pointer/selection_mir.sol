//@ compile-flags: -O none -Zdump=mir
//@ filecheck:

contract FunctionPointerSelection {
    // CHECK-LABEL: fn @choose(
    // CHECK: [[FN:v[0-9]+]] = phi [bb1: 2], [bb2: 3]
    // CHECK: internal_call @internal_dispatcher{{.*}}, 1, [[FN]], arg1
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
    // CHECK: internal_call @internal_dispatcher{{.*}}, 1, 2, arg0
    function callConstant(uint256 value) public returns (uint256) {
        function(uint256) internal returns (uint256) fn = increment;
        return fn(value);
    }

    // CHECK-LABEL: fn @throughCast(
    // CHECK: [[CASTED:v[0-9]+]] = internal_call @castViewToPure, 1, 7
    // CHECK: internal_call @internal_dispatcher{{.*}}, 1, [[CASTED]], arg0
    // CHECK-LABEL: fn @castViewToPure(
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

    // CHECK-LABEL: fn @internal_dispatcher{{.*}}(
    // CHECK: eq arg0, 2
    // CHECK: eq arg0, 3
    // CHECK: eq arg0, 7
    // CHECK: mstore 4, 81
    // CHECK: internal_call @incrementView, 1, arg1
    // CHECK: internal_call @decrement, 1, arg1
    // CHECK: internal_call @increment, 1, arg1
}
