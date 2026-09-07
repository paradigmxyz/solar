//@ codegen-matrix: standard ir raw
//@[ir] filecheck:
//@[ir] compile-flags: -Osize -Zdump=evm-ir-runtime
//@[raw] filecheck:
//@[raw] compile-flags: -Ogas -Zdump=evm-ir-runtime -Zmir-pipeline=lower-abi,lower-dispatch,lower-frame-slots,lower-memory-objects,lower-alloc,lower-evm-shaped

//@ run-call-fail: DescendantLoad::run 1024 => 0x00000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: DescendantByte::run 7 => 0x07000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000007
//@ run-call-fail: DescendantCopy::run 7 => 0x00000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000007
//@ run-call-fail: DescendantLog::run 7 => 0x0000000000000000000000000000000000000000000000000000000000000007
//@ run-call-fail: DescendantMsize::run 7 => 0x00000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: DescendantCall::run 7 => 0x000000000000000000000000000000000000000000000000000000000000000700000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000001

//@ run-call-fail: TailClosureCycle::run 0, 7 => 0x0000000000000000000000000000000000000000000000000000000000000007
//@ run-call-fail: TailClosureCycle::run 3, 7 => 0x0000000000000000000000000000000000000000000000000000000000000007
// The observation is in a descendant, after an otherwise pure branching parent.
// Raw lowering preserves the call boundary when ordinary inlining removes it.

// CHECK-LABEL: @module DescendantLoad_runtime
// CHECK: push 192
// CHECK-NEXT: mstore
// CHECK: {{push 288[[:space:]]+mload}}
// CHECK: mload
contract DescendantLoad {
    function run(uint256 a) external pure { route(a); }
    function other(uint256 a) external pure { route(a); }
    function route(uint256 a) internal pure {
        if (a == 0) leaf(1);
        leaf(a);
        revert();
    }
    function leaf(uint256 a) internal pure {
        assembly { let observed := mload(a) mstore(0, a) mstore(32, observed) revert(0, 64) }
    }
}

// CHECK-LABEL: @module DescendantByte_runtime
// CHECK: push 192
// CHECK-NEXT: mstore
// CHECK: {{push 288[[:space:]]+mload}}
// CHECK: mstore8
contract DescendantByte {
    function run(uint256 a) external pure { route(a); }
    function other(uint256 a) external pure { route(a); }
    function route(uint256 a) internal pure {
        if (a == 0) leaf(1);
        leaf(a);
        revert();
    }
    function leaf(uint256 a) internal pure {
        assembly { mstore8(0, a) mstore(32, a) revert(0, 64) }
    }
}

// CHECK-LABEL: @module DescendantCopy_runtime
// CHECK: push 192
// CHECK-NEXT: mstore
// CHECK: {{push 288[[:space:]]+mload}}
// CHECK: mcopy
contract DescendantCopy {
    function run(uint256 a) external pure { route(a); }
    function other(uint256 a) external pure { route(a); }
    function route(uint256 a) internal pure {
        if (a == 0) leaf(1);
        leaf(a);
        revert();
    }
    function leaf(uint256 a) internal pure {
        assembly { mstore(0, a) mcopy(32, 0, 32) revert(0, 64) }
    }
}

// CHECK-LABEL: @module DescendantLog_runtime
// CHECK: push 192
// CHECK-NEXT: mstore
// CHECK: {{push 288[[:space:]]+mload}}
// CHECK: log0
contract DescendantLog {
    function run(uint256 a) external  { route(a); }
    function other(uint256 a) external  { route(a); }
    function route(uint256 a) internal  {
        if (a == 0) leaf(1);
        leaf(a);
        revert();
    }
    function leaf(uint256 a) internal  {
        assembly { mstore(0, a) log0(0, 32) revert(0, 32) }
    }
}

// CHECK-LABEL: @module DescendantMsize_runtime
// CHECK: push 192
// CHECK-NEXT: mstore
// CHECK: {{push 288[[:space:]]+mload}}
// CHECK: msize
contract DescendantMsize {
    function run(uint256 a) external pure { route(a); }
    function other(uint256 a) external pure { route(a); }
    function route(uint256 a) internal pure {
        if (a == 0) leaf(1);
        leaf(a);
        revert();
    }
    function leaf(uint256 a) internal pure {
        assembly { mstore(0, a) let observed := msize() mstore(32, gt(observed, 0)) revert(0, 64) }
    }
}

// CHECK-LABEL: @module DescendantCall_runtime
// CHECK: push 192
// CHECK-NEXT: mstore
// CHECK: {{push 288[[:space:]]+mload}}
// CHECK: staticcall
contract DescendantCall {
    function run(uint256 a) external view { route(a); }
    function other(uint256 a) external view { route(a); }
    function route(uint256 a) internal view {
        if (a == 0) leaf(1);
        leaf(a);
        revert();
    }
    function leaf(uint256 a) internal view {
        assembly { mstore(0, a) let ok := staticcall(50000, 4, 0, 32, 32, 32) mstore(64, ok) revert(0, 96) }
    }
}

// CHECK-LABEL: @module TailClosureCycle_runtime
// CHECK: push 192
// CHECK-NEXT: mstore
// CHECK: {{push 192[[:space:]]+mload}}
contract TailClosureCycle {
    function run(uint256 n, uint256 a) external pure { route(n, a); }
    function other(uint256 n, uint256 a) external pure { route(n, a); }
    function route(uint256 n, uint256 a) internal pure {
        assembly {
            for { } n { n := sub(n, 1) } { a := xor(a, n) }
        }
        leaf(a);
        revert();
    }
    function leaf(uint256 a) internal pure {
        assembly { mstore(0, a) revert(0, 32) }
    }
}

//@ run-call-fail: DescendantHash::run 7 => 0x0000000000000000000000000000000000000000000000000000000000000007a66cc928b5edb82af9bd49922954155ab7b0942694bea4ce44661d9a8736c688
// The descendant observation must prevent the ancestor's entry bypass.
// CHECK-LABEL: @module DescendantHash_runtime
// CHECK: push 192
// CHECK-NEXT: mstore
// CHECK: {{push 288[[:space:]]+mload}}
// CHECK: keccak256
contract DescendantHash {
    function run(uint256 a) external pure { route(a); }
    function other(uint256 a) external pure { route(a); }
    function route(uint256 a) internal pure {
        if (a == 0) leaf(1);
        leaf(a);
        revert();
    }
    function leaf(uint256 a) internal pure {
        assembly { mstore(0, a) mstore(32, keccak256(0, 32)) revert(0, 64) }
    }

}

//@ run-call-fail: DescendantGas::run 7 => 0x00000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000001
// The descendant observation must prevent the ancestor's entry bypass.
// CHECK-LABEL: @module DescendantGas_runtime
// CHECK: push 192
// CHECK-NEXT: mstore
// CHECK: {{push 288[[:space:]]+mload}}
// CHECK: gas
contract DescendantGas {
    function run(uint256 a) external view { route(a); }
    function other(uint256 a) external view { route(a); }
    function route(uint256 a) internal view {
        if (a == 0) leaf(1);
        leaf(a);
        revert();
    }
    function leaf(uint256 a) internal view {
        assembly { mstore(0, a) mstore(32, gt(gas(), 0)) revert(0, 64) }
    }
}
