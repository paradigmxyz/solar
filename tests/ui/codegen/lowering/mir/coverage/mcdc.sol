//@compile-flags: -O none -Zinstrument-coverage -Zcoverage-mcdc -Zdump=mir
//@filecheck: --enable-var-scope

// An MC/DC probe is `coverage <probe>` with bit 31 set: bits 30 and 29 are the operation, `0` to
// begin, `1` to add, and `2` to end, bits 28 to 8 the decision, and bits 7 to 0 the weight of an
// add. A decision begins before its first condition and ends once its value is known. The true
// outcome of a condition adds the paths that its false outcome leaves, so the false outcome adds
// nothing. `(a && b) || c` has 5 test vectors, and the weights of `a`, `b`, and `c` are 2, 2, and 1.
contract Mcdc {
    // CHECK-LABEL: fn @three(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage 1
    // CHECK-NEXT: coverage 2147483648
    // CHECK-NEXT: coverage 2
    // CHECK-NEXT: jumpi arg0, [[B:bb[0-9]+]],
    // CHECK: [[B]]:
    // The weight of `a` is on the path to `b`.
    // CHECK-NEXT: coverage 2684354562
    // CHECK-NEXT: coverage 3
    // CHECK-NEXT: coverage 4
    // CHECK-NEXT: jumpi arg1, [[BTRUE:bb[0-9]+]],
    // The right operand of `&&` adds its weight on the path where it holds, whose value joins the
    // paths again.
    // CHECK: [[BTRUE]]:
    // CHECK-NEXT: coverage 2684354562
    // The weight of `c` is on its true path, and the decision ends where its value is known.
    // CHECK: coverage 2684354561
    // CHECK: coverage 3221225472
    function three(bool a, bool b, bool c) external pure returns (uint256 r) {
        if ((a && b) || c) {
            r = 1;
        }
    }

    // A decision that no branch consumes has probes too.
    // CHECK-LABEL: fn @value(
    // CHECK: coverage 2147483904
    // CHECK: coverage 2684354817
    // CHECK: coverage 2684354817
    // CHECK: coverage 3221225728
    function value(bool a, bool b) external pure returns (bool) {
        return a && b;
    }

    // A single condition is no MC/DC decision.
    // CHECK-LABEL: fn @single(
    // CHECK-NOT: coverage 2
    function single(bool a) external pure returns (uint256 r) {
        if (a) {
            r = 1;
        }
    }
}
