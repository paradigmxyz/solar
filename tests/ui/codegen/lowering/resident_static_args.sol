//@ revisions: ir run
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[run] compile-flags: -Ogas
//@ run-call: run 40, false => 126
//@ run-call: run 40, true => 123
//@ run-call: checked 40, false => 42
//@ run-call-fail: checked 40, true => 0xc77ea6410000000000000000000000000000000000000000000000000000000000000028
//@ run-call: nested 40, false => 82
//@ run-call: nested 40, true => 81
//@ run-call: asymmetric 40, false => 7
//@ run-call: asymmetric 40, true => 80
//@ run-call: joined 40, false => 82
//@ run-call: joined 40, true => 81

contract ResidentStaticArgs {
    error Failed(uint256 value);

    // The call pushes its return label and both calldata arguments directly;
    // the branch join reuses `value` from the resident stack without a frame load.
    // CHECK: push [[RET:bb[0-9]+]]
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK: [[JOIN:bb[0-9]+]]:
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: add
    function run(uint256 value, bool first) external pure returns (uint256) {
        return choose(value, first) * 3;
    }

    function choose(uint256 value, bool first) internal pure returns (uint256) {
        if (first) {
            return value + 1;
        }
        return value + 2;
    }

    function checked(uint256 value, bool fail) external pure returns (uint256) {
        return chooseOrRevert(value, fail);
    }

    function nested(uint256 value, bool first) external pure returns (uint256) {
        return retainAcrossCall(value, first);
    }

    // `value` remains resident below the nested call's return address and is
    // reused after `choose` returns.
    function retainAcrossCall(uint256 value, bool first) internal pure returns (uint256) {
        return choose(value, first) + value;
    }

    function asymmetric(uint256 value, bool useValue) external pure returns (uint256) {
        return chooseOrDefault(value, useValue);
    }

    // Both arms enter with the same physical layout. The false arm discards
    // its dead resident word before returning the constant.
    function chooseOrDefault(uint256 value, bool useValue) internal pure returns (uint256) {
        if (useValue) {
            return value + value;
        }
        return 7;
    }

    function joined(uint256 value, bool first) external pure returns (uint256) {
        return retainAcrossJoin(value, first);
    }

    // The selected constant changes across the join's phi while `value` keeps
    // the same resident identity below it.
    function retainAcrossJoin(uint256 value, bool first) internal pure returns (uint256) {
        uint256 selected;
        if (first) {
            selected = 1;
        } else {
            selected = 2;
        }
        return value + value + selected;
    }

    // The revert successor consumes `value`, so its resident entry layout must
    // agree exactly with the non-terminal branch.
    function chooseOrRevert(uint256 value, bool fail) internal pure returns (uint256) {
        if (fail) {
            revert Failed(value);
        }
        return value + 2;
    }
}
