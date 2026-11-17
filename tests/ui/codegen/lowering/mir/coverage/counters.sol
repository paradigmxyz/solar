//@compile-flags: -O none -Zinstrument-coverage -Zdump=mir
//@filecheck: --enable-var-scope

// Counters number functions, statements, and decisions in source order, and a modifier counts in
// each function that invokes it. A leaf condition counts once it is evaluated, before the branch
// on it, and a path counts at its start. The first statement of a path shares the count of the
// path.
contract Counters {
    uint256 public total;

    modifier nonZero(uint256 a) {
        require(a != 0);
        _;
    }

    // CHECK-LABEL: fn @branches(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage 1
    // CHECK-NEXT: coverage 2
    // CHECK-NEXT: ne arg0, 0
    // CHECK-NEXT: coverage 3
    // CHECK: icall revert_if<empty>
    // CHECK-NEXT: coverage 4
    // CHECK-NEXT: coverage 5
    // CHECK-NEXT: coverage 6
    // CHECK-NEXT: [[LHS:v[0-9]+]] = gt arg0, 10
    // CHECK-NEXT: coverage 7
    // CHECK-NEXT: jumpi [[LHS]], [[RHS:bb[0-9]+]], [[SHORT:bb[0-9]+]]
    // CHECK: [[RHS]]:
    // CHECK-NEXT: coverage 8
    // CHECK-NEXT: coverage 9
    // CHECK: [[COND:v[0-9]+]] = phi
    // CHECK-NEXT: jumpi [[COND]], [[THEN:bb[0-9]+]], [[ELSE:bb[0-9]+]]
    // CHECK: [[ELSE]]:
    // CHECK-NEXT: coverage 11
    // CHECK-NEXT: jumpi arg1, [[TRUE:bb[0-9]+]], [[FALSE:bb[0-9]+]]
    // CHECK: [[TRUE]]:
    // CHECK-NEXT: coverage 12
    // CHECK: [[THEN]]:
    // CHECK-NEXT: coverage 10
    function branches(uint256 a, bool b) external pure nonZero(a) returns (uint256 r) {
        if (a > 10 && b) {
            r = 1;
        } else {
            r = b ? 2 : 3;
        }
    }

    // CHECK-LABEL: fn @loops(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage 13
    // CHECK: [[COND:v[0-9]+]] = lt
    // CHECK-NEXT: coverage 14
    // CHECK-NEXT: jumpi [[COND]], [[BODY:bb[0-9]+]], [[EXIT:bb[0-9]+]]
    // CHECK: coverage 17
    // CHECK: coverage 18
    // CHECK-NEXT: checked_sub
    // CHECK: [[WHILE:v[0-9]+]] = gt
    // CHECK-NEXT: coverage 19
    // CHECK-NEXT: jumpi [[WHILE]], [[AGAIN:bb[0-9]+]],
    // CHECK: [[AGAIN]]:
    // CHECK-NEXT: coverage 20
    // CHECK: [[BODY]]:
    // CHECK-NEXT: coverage 15
    // CHECK: coverage 16
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
    // CHECK-NEXT: coverage 21
    // CHECK-NEXT: coverage 22
    // CHECK-NEXT: switch arg0, default [[MERGE:bb[0-9]+]], [1 => [[ONE:bb[0-9]+]], 2 => [[TWO:bb[0-9]+]]]
    // CHECK: [[TWO]]:
    // CHECK-NEXT: coverage 24
    // CHECK: [[ONE]]:
    // CHECK-NEXT: coverage 23
    function cases(uint256 a) external pure returns (uint256 r) {
        assembly {
            switch a
            case 1 { r := 7 }
            case 2 { r := 8 }
        }
    }

    // The returns clause counts once its data decodes, and the call counts when it returns.
    // CHECK-LABEL: fn @clauses(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage 25
    // CHECK: staticcall
    // CHECK-NEXT: coverage 26
    // CHECK: coverage 28
    // CHECK: icall revert_if<tuple_data_too_short>
    // CHECK: coverage 27
    function clauses(Counters c) external pure returns (bool ok) {
        try c.cases(1) returns (uint256) {
            ok = true;
        } catch {
            ok = false;
        }
    }

    // A decision in a modifier argument counts before the modifier runs.
    // CHECK-LABEL: fn @modified(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage 29
    // CHECK-NEXT: coverage 30
    // CHECK-NEXT: jumpi arg0, [[TRUE:bb[0-9]+]],
    // CHECK: [[TRUE]]:
    // CHECK-NEXT: coverage 31
    // CHECK: phi
    // CHECK-NEXT: zext
    // CHECK-NEXT: coverage 32
    function modified(bool b) external pure nonZero(b ? 1 : 2) {}

    // Fusing the packed encoding into the hash keeps both statement counters.
    // CHECK-LABEL: fn @packed(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage 36
    // CHECK-NEXT: keccak256_packed
    // CHECK-NEXT: coverage 37
    // CHECK-NEXT: ret
    function packed(uint256 x) external pure returns (bytes32) {
        bytes memory p = abi.encodePacked(x);
        return keccak256(p);
    }
}
