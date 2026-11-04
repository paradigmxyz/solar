//@ codegen-matrix: standard hot
//@[hot] compile-flags: -Ogas -Zdump=mir
//@[hot] filecheck:
//@ run-call: bare 0 => 2
//@ run-call: bare 1 => 100
//@ run-call: typed 0 => 3
//@ run-call: typed 1 => 4
//@ run-call: typed 2 => 5
//@ run-call: looped 0 => 11
//@ run-call: looped 1 => 12
//@ run-call: looped 2 => 9

// The catch clauses of a `try` run rarely, as the unwind path of an `invoke` does: lowering gives
// the blocks of the catch side `cold_hint`, so they are cold, and the success path and the join
// stay hot.

contract C {
    function check(uint256 x) external pure returns (uint256) {
        if (x == 1) revert();
        if (x == 2) revert("no");
        return x + 1;
    }

    // CHECK-LABEL: fn @bare()
    // CHECK: staticcall
    // CHECK-NEXT: jumpi {{v[0-9]+}}, [[OK:bb[0-9]+]], [[CATCH:bb[0-9]+]]
    // CHECK: [[CATCH]] [cold, cold_hint]:
    // CHECK: [[OK]]:
    function bare(uint256 x) public view returns (uint256) {
        try this.check(x) returns (uint256 y) {
            return y + 1;
        } catch {
            return 100;
        }
    }

    // CHECK-LABEL: fn @typed()
    // CHECK: staticcall
    // CHECK-NEXT: jumpi {{v[0-9]+}}, [[OK:bb[0-9]+]], [[CATCH:bb[0-9]+]]
    // CHECK: [[CATCH]] [cold, cold_hint]:
    // CHECK: [[OK]]:
    // CHECK: [[JOIN:bb[0-9]+]]:  ; preds = {{.*}}, {{.*}}
    // CHECK-NEXT: {{v[0-9]+}} = phi
    function typed(uint256 x) public view returns (uint256) {
        uint256 r;
        try this.check(x) returns (uint256 y) {
            r = y + 2;
        } catch Error(string memory reason) {
            r = bytes(reason).length + 3;
        } catch {
            r = 4;
        }
        return r;
    }

    // CHECK-LABEL: fn @looped()
    // CHECK: staticcall
    // CHECK-NEXT: jumpi {{v[0-9]+}}, [[OK:bb[0-9]+]], [[CATCH:bb[0-9]+]]
    // CHECK: [[CATCH]] [cold, cold_hint]:
    // CHECK: [[LOOP:bb[0-9]+]] [cold, cold_hint]:  ; preds = [[CATCH]], {{.*}}
    // CHECK-NEXT: {{v[0-9]+}} = phi
    // CHECK: [[OK]]:
    // CHECK: [[JOIN:bb[0-9]+]]:  ; preds = {{.*}}, {{.*}}
    // CHECK-NEXT: {{v[0-9]+}} = phi
    function looped(uint256 x) public view returns (uint256) {
        uint256 r = x;
        try this.check(x + 1) returns (uint256 y) {
            r = y;
        } catch {
            for (uint256 i; i < 6; ++i) {
                r += 1;
            }
        }
        return r + 5;
    }
}
