//@ revisions: built evm_shaped
//@[built] compile-flags: -Zcodegen -O none -Zdump=mir
//@[built] filecheck: --check-prefix=BUILT
//@[evm_shaped] compile-flags: -Zcodegen -Ogas -Zdump=mir-evm-shaped
//@[evm_shaped] filecheck: --check-prefix=SHAPED

// BUILT-LABEL: fn @constructor(
// BUILT: sstore 1, [[SET_FLAG:[0-9]+]]
// SHAPED: @phase evm-shaped
contract InternalFunctionPointerMir {
    bool flag;
    function() internal stateFn = setFlag;

    // BUILT-LABEL: fn @dynamic(
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
    // SHAPED-LABEL: fn @dynamic(
    // SHAPED: internal_call @__internal_dispatch_0, 1, {{v[0-9]+}}, arg1
    // SHAPED-LABEL: fn @__internal_dispatch_0(
    // SHAPED: eq arg0, 2
    // SHAPED: eq arg0, 3
    // SHAPED: eq arg0, 7
    // SHAPED: tail_call @[[INVALID:__revert_stub[0-9]+]]
    function dynamic(bool add, uint256 value) public returns (uint256) {
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
    // SHAPED-LABEL: fn @callConstant(
    // SHAPED-NOT: internal_call @__internal_dispatch
    // SHAPED: add arg0, 1
    // SHAPED: returndata 128, 32
    function callConstant(uint256 value) public returns (uint256) {
        function(uint256) internal returns (uint256) fn = increment;
        return fn(value);
    }

    // BUILT-LABEL: fn @callCast(
    // BUILT: [[CASTED:v[0-9]+]] = internal_call @castViewToPure, 1, [[INCREMENT_VIEW]]
    // BUILT: internal_call @__internal_dispatch_0, 1, [[CASTED]], arg0
    // BUILT-LABEL: fn @castViewToPure(
    // BUILT: mstore {{v[0-9]+}}, arg0
    // BUILT: ret {{v[0-9]+}}
    // SHAPED-LABEL: fn @callCast(
    // SHAPED-NOT: internal_call @__internal_dispatch
    // SHAPED: number
    // SHAPED: add arg0, 1
    function callCast(uint256 value) public pure returns (uint256) {
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

    // BUILT-LABEL: fn @callVoid(
    // BUILT: internal_call @__internal_dispatch_1, 0, [[SET_FLAG]]
    // BUILT-LABEL: fn @__internal_dispatch_1(
    // BUILT: eq arg0, [[SET_FLAG]]
    // BUILT: internal_call setFlag{{[0-9]+}}, 0
    // BUILT: mstore 4, 81
    // SHAPED-LABEL: fn @callVoid(
    // SHAPED-NOT: internal_call @__internal_dispatch
    // SHAPED: internal_call setFlag{{[0-9]+}}, 0
    // SHAPED: returndata 128, 32
    // SHAPED-LABEL: fn @__internal_dispatch_1(
    // SHAPED: eq arg0, 9
    // SHAPED: tail_call @[[INVALID]]
    function callVoid() public returns (bool) {
        function() internal fn = setFlag;
        fn();
        return flag;
    }

    function setFlag() public {
        flag = true;
    }

    // BUILT-LABEL: fn @callState(
    // BUILT: [[STORED:v[0-9]+]] = sload 1
    // BUILT: internal_call @__internal_dispatch_1, 0, [[STORED]]
    // SHAPED-LABEL: fn @callState(
    // SHAPED: [[STORED:v[0-9]+]] = sload 1
    // SHAPED: internal_call @__internal_dispatch_1, 0, [[STORED]]
    function callState() public returns (bool) {
        stateFn();
        return flag;
    }

    // BUILT-LABEL: fn @callPair(
    // BUILT: internal_call @__internal_dispatch_2, 2, [[PAIR:[0-9]+]], arg0
    // BUILT-LABEL: fn @__internal_dispatch_2(
    // BUILT: eq arg0, [[PAIR]]
    // BUILT: [[FIRST:v[0-9]+]] = internal_call @pair, 2, arg1
    // BUILT: [[SECOND:v[0-9]+]] = mload {{v[0-9]+}}
    // BUILT: ret [[FIRST]], [[SECOND]]
    // SHAPED-LABEL: fn @callPair(
    // SHAPED-NOT: internal_call @__internal_dispatch
    // SHAPED: add arg0, 1
    // SHAPED: mstore 128, arg0
    // SHAPED: mstore 160, {{v[0-9]+}}
    // SHAPED: returndata 128, 64
    function callPair(uint256 value) public returns (uint256, uint256) {
        function(uint256) internal returns (uint256, uint256) fn = pair;
        return fn(value);
    }

    function pair(uint256 value) internal pure returns (uint256, uint256) {
        return (value, value + 1);
    }

    // BUILT-LABEL: fn @callZero(
    // BUILT: internal_call @__internal_dispatch_1, 0, 0
    // SHAPED-LABEL: fn @callZero(
    // SHAPED-NEXT: bb0:
    // SHAPED-NEXT: tail_call @[[INVALID]]
    // SHAPED: fn @[[INVALID]](
    // SHAPED: mstore 4, 81
    // SHAPED: fn @__revert_stub{{[0-9]+}}(
    // SHAPED: mstore 4, 17
    function callZero() public {
        function() internal fn;
        fn();
    }
}
