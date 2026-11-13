//@compile-flags: -O none -Zinstrument-coverage -Zdump=mir
//@filecheck: --enable-var-scope

// Counters number functions, statements, and decisions in source order, so the modifier comes
// first. A decision counts once its condition is evaluated, and its true path counts at its
// start.
contract Counters {
    uint256 public total;

    modifier nonZero(uint256 a) {
        require(a != 0);
        _;
    }

    // CHECK-LABEL: fn @branches(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage 5
    // CHECK-NEXT: coverage 1
    // CHECK-NEXT: coverage 2
    // CHECK-NEXT: ne arg0, 0
    // CHECK-NEXT: coverage 3
    // CHECK: icall revert_if<empty>
    // CHECK-NEXT: coverage 4
    // CHECK-NEXT: coverage 6
    // CHECK-NEXT: [[LHS:v[0-9]+]] = gt arg0, 10
    // CHECK-NEXT: coverage 7
    // CHECK-NEXT: jumpi [[LHS]], [[RHS:bb[0-9]+]], [[SHORT:bb[0-9]+]]
    // CHECK: [[RHS]]:
    // CHECK-NEXT: coverage 8
    // CHECK: [[COND:v[0-9]+]] = phi
    // CHECK-NEXT: coverage 9
    // CHECK-NEXT: jumpi [[COND]], [[THEN:bb[0-9]+]], [[ELSE:bb[0-9]+]]
    // CHECK: [[ELSE]]:
    // CHECK-NEXT: coverage 12
    // CHECK-NEXT: coverage 13
    // CHECK-NEXT: jumpi arg1, [[TRUE:bb[0-9]+]], [[FALSE:bb[0-9]+]]
    // CHECK: [[TRUE]]:
    // CHECK-NEXT: coverage 14
    // CHECK: [[THEN]]:
    // CHECK-NEXT: coverage 10
    // CHECK-NEXT: coverage 11
    function branches(uint256 a, bool b) external pure nonZero(a) returns (uint256 r) {
        if (a > 10 && b) {
            r = 1;
        } else {
            r = b ? 2 : 3;
        }
    }

    // CHECK-LABEL: fn @loops(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage 15
    // CHECK-NEXT: coverage 16
    // CHECK-NEXT: coverage 17
    // CHECK: [[COND:v[0-9]+]] = lt
    // CHECK-NEXT: coverage 18
    // CHECK-NEXT: jumpi [[COND]], [[BODY:bb[0-9]+]], [[EXIT:bb[0-9]+]]
    // CHECK: coverage 22
    // CHECK: coverage 23
    // CHECK-NEXT: checked_sub
    // CHECK: [[WHILE:v[0-9]+]] = gt
    // CHECK-NEXT: coverage 24
    // CHECK-NEXT: jumpi [[WHILE]], [[AGAIN:bb[0-9]+]],
    // CHECK: [[AGAIN]]:
    // CHECK-NEXT: coverage 25
    // CHECK: [[BODY]]:
    // CHECK-NEXT: coverage 19
    // CHECK-NEXT: coverage 20
    // CHECK: coverage 21
    // CHECK-NEXT: checked_add
    function loops(uint256 n) external {
        for (uint256 i = 0; i < n; i++) {
            total += i;
        }
        do {
            n--;
        } while (n > 0);
    }

    // CHECK-LABEL: fn @cases(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage 26
    // CHECK-NEXT: coverage 27
    // CHECK-NEXT: coverage 28
    // CHECK-NEXT: coverage 29
    // CHECK-NEXT: switch arg0, default [[MERGE:bb[0-9]+]], [1 => [[ONE:bb[0-9]+]], 2 => [[TWO:bb[0-9]+]]]
    // CHECK: [[TWO]]:
    // CHECK-NEXT: coverage 31
    // CHECK-NEXT: coverage 33
    // CHECK: [[ONE]]:
    // CHECK-NEXT: coverage 30
    // CHECK-NEXT: coverage 32
    function cases(uint256 a) external pure returns (uint256 r) {
        assembly {
            switch a
            case 1 { r := 7 }
            case 2 { r := 8 }
        }
    }

    // The returns clause counts once its data decodes.
    // CHECK-LABEL: fn @clauses(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage 34
    // CHECK-NEXT: coverage 35
    // CHECK: coverage 37
    // CHECK-NEXT: coverage 39
    // CHECK: icall revert_if<tuple_data_too_short>
    // CHECK: coverage 36
    // CHECK-NEXT: coverage 38
    function clauses(Counters c) external pure returns (bool ok) {
        try c.cases(1) returns (uint256) {
            ok = true;
        } catch {
            ok = false;
        }
    }
}
