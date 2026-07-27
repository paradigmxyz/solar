//@ revisions: built optimized
//@[built] compile-flags: -Zcodegen -Zdump=mir
//@[built] filecheck: --check-prefix=BUILT
//@[optimized] compile-flags: -Zcodegen -Ogas -Zdump=evm-ir-runtime
//@[optimized] filecheck: --check-prefix=EVMIR

// BUILT-LABEL: fn @constructor(
// BUILT: sstore 1, [[SET_FLAG:[0-9]+]]
// EVMIR-LABEL: @module runtime
// EVMIR: push 0x560cf94
// EVMIR: eq
// EVMIR-NEXT: push [[CONSTANT:bb[0-9]+]]
// EVMIR-NEXT: jumpi
// EVMIR: push 0x46e2b562
// EVMIR: eq
// EVMIR-NEXT: push [[INVALID:bb[0-9]+]]
// EVMIR-NEXT: jumpi
// EVMIR: push 0x85ead1f5
// EVMIR: eq
// EVMIR-NEXT: push [[PAIR:bb[0-9]+]]
// EVMIR-NEXT: jumpi
// EVMIR: push 0xb67d6f95
// EVMIR: eq
// EVMIR-NEXT: push [[STATE:bb[0-9]+]]
// EVMIR-NEXT: jumpi
// EVMIR: push 0xc1419383
// EVMIR: eq
// EVMIR-NEXT: push [[DYNAMIC:bb[0-9]+]]
// EVMIR-NEXT: jumpi
// EVMIR: push 0xd04622c6
// EVMIR: eq
// EVMIR-NEXT: push [[VOID:bb[0-9]+]]
// EVMIR-NEXT: jumpi
// EVMIR: push 0xe09cd08b
// EVMIR: eq
// EVMIR-NEXT: push [[CAST:bb[0-9]+]]
// EVMIR-NEXT: jumpi
// EVMIR: [[INVALID]] [cold]:
// EVMIR: push 0x4e487b71
// EVMIR: mstore
// EVMIR: push 81
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
    // EVMIR: [[DYNAMIC]]:
    // EVMIR: push 3
    // EVMIR-NEXT: jump [[DISPATCH:bb[0-9]+]]
    // EVMIR: [[DISPATCH]]:
    // EVMIR: push 2
    // EVMIR: eq
    // EVMIR: push 3
    // EVMIR: push [[INVALID]]
    // EVMIR: jumpi
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
    // EVMIR: [[CONSTANT]]:
    // EVMIR: push 1
    // EVMIR-NEXT: push 4
    // EVMIR-NEXT: calldataload
    // EVMIR-NEXT: add
    // EVMIR: jump {{bb[0-9]+}}
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
    // EVMIR: [[CAST]]:
    // EVMIR: number
    // EVMIR: push 1
    // EVMIR: add
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
    // BUILT: internal_call fn{{[0-9]+}}, 0
    // BUILT: mstore 4, 81
    // EVMIR: [[VOID]]:
    // EVMIR: push [[VOID_CONT:bb[0-9]+]]
    // EVMIR-NEXT: jump [[SET_FLAG:bb[0-9]+]]
    // EVMIR: [[SET_FLAG]]:
    // EVMIR: sstore
    // EVMIR: jump
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
    // EVMIR: [[STATE]]:
    // EVMIR: {{^  push 1$}}
    // EVMIR-NEXT: sload
    // EVMIR: push 9
    // EVMIR: sub
    // EVMIR: push [[INVALID]]
    // EVMIR-NEXT: jumpi
    // EVMIR: jump [[SET_FLAG]]
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
    // EVMIR: [[PAIR]]:
    // EVMIR: push 1
    // EVMIR-NEXT: push 4
    // EVMIR-NEXT: calldataload
    // EVMIR-NEXT: add
    // EVMIR: push 128
    // EVMIR-NEXT: mstore
    // EVMIR: push 160
    // EVMIR-NEXT: mstore
    // EVMIR: return
    function callPair(uint256 value) public returns (uint256, uint256) {
        function(uint256) internal returns (uint256, uint256) fn = pair;
        return fn(value);
    }

    function pair(uint256 value) internal pure returns (uint256, uint256) {
        return (value, value + 1);
    }

    // BUILT-LABEL: fn @callZero(
    // BUILT: internal_call @__internal_dispatch_1, 0, 0
    function callZero() public {
        function() internal fn;
        fn();
    }
}
