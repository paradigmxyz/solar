//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: fill 1 => 0x10000000000000000000000000000010001
//@ run-call: callBetween 1 => 0x10000000000000000000000000000050001
//@ run-call: twoSlots 1, 1 => 0x70101, 0x70101
//@ run-call: twoSlots 1, 2 => 0x70001, 0x100
//@ run-call: guarded 1, false => 0x10000000000000000000000000000010001
//@ run-call-fail: guarded 1, true => 0x

// Writes to the fields of one packed slot, as in Seaport's order status update.
// Storage CSE forwards each written word to the next field's read, storage DSE
// drops the overwritten stores and the e-graph folds the masks, so one store
// remains; when every bit comes from constants, the load goes too. Anything
// that may read the slot in between, or may write it through another key,
// keeps the writes apart.
contract PackedWrites {
    struct OrderStatus {
        bool isValidated;
        bool isCancelled;
        uint120 numerator;
        uint120 denominator;
    }

    mapping(uint256 => OrderStatus) internal statuses;

    // CHECK-LABEL: fn @fill()
    // CHECK-NOT: sload
    // CHECK: sstore {{.*}}, 0x10000000000000000000000000000010001
    // CHECK-NOT: sstore
    // CHECK: staticcall
    function fill(uint256 h) external returns (uint256) {
        OrderStatus storage status = statuses[h];
        status.isValidated = true;
        status.isCancelled = false;
        status.numerator = 1;
        status.denominator = 1;
        return this.raw(h);
    }

    // The call reads the slot, so the write before it stays.
    // CHECK-LABEL: fn @callBetween()
    // CHECK: sstore
    // CHECK: staticcall
    // CHECK: sstore
    // CHECK-NOT: sstore
    // CHECK: staticcall
    function callBetween(uint256 h) external returns (uint256) {
        OrderStatus storage status = statuses[h];
        status.isValidated = true;
        status.numerator = uint120(this.peek(h));
        status.denominator = 1;
        return this.raw(h);
    }

    // `a` and `b` may name the same slot, so the write through `b` separates the
    // writes through `a`.
    // CHECK-LABEL: fn @twoSlots()
    // CHECK: sstore
    // CHECK: sstore
    // CHECK: sstore
    // CHECK: staticcall
    function twoSlots(uint256 a, uint256 b) external returns (uint256, uint256) {
        OrderStatus storage x = statuses[a];
        OrderStatus storage y = statuses[b];
        x.isValidated = true;
        y.isCancelled = true;
        x.numerator = 7;
        return (this.raw(a), this.raw(b));
    }

    // `gasCheck` reads gas, but `bubble` reverts after it, so no gas read comes
    // after the writes and they still merge.
    // CHECK-LABEL: fn @guarded()
    // CHECK: sload
    // CHECK: sstore {{.*}}, 0x10000000000000000000000000000010001
    // CHECK-NOT: sstore
    // CHECK: staticcall
    function guarded(uint256 h, bool fail) external returns (uint256) {
        OrderStatus storage status = statuses[h];
        if (!status.isValidated) bubble(fail);
        status.isValidated = true;
        status.isCancelled = false;
        status.numerator = 1;
        status.denominator = 1;
        return this.raw(h);
    }

    // Between two gas reads, every access stays for the second read to observe.
    // CHECK-LABEL: fn @measured()
    // CHECK: = gas
    // CHECK-COUNT-4: sstore
    // CHECK: = gas
    function measured(uint256 h) external returns (uint256) {
        OrderStatus storage status = statuses[h];
        uint256 g = gasleft();
        status.isValidated = true;
        status.isCancelled = false;
        status.numerator = 1;
        status.denominator = 1;
        return g - gasleft();
    }

    function peek(uint256 h) external view returns (uint256) {
        return statuses[h].isValidated ? 5 : 3;
    }

    function raw(uint256 h) external view returns (uint256 word) {
        OrderStatus storage status = statuses[h];
        assembly {
            word := sload(status.slot)
        }
    }

    /// @custom:solar-inline never
    function bubble(bool fail) internal view {
        if (fail) {
            gasCheck();
            revert();
        }
    }

    /// @custom:solar-inline never
    function gasCheck() internal view {
        assembly {
            if lt(gas(), 10) { revert(0, 0) }
        }
    }
}
