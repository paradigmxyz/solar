//@compile-flags: -Ogas -Zinstrument-coverage -Zdump=mir-final
//@filecheck: --enable-var-scope

// Counters must survive optimization: a coverage build validates the IR after every pass, so the
// glued `push; coverage` pair is checked through the gas pipeline (loops, calls, dedup, outlining).
contract C {
    uint256 total;

    function add(uint256 x) internal {
        total += x;
    }

    // CHECK-LABEL: fn @loop
    // CHECK: coverage
    // CHECK: [[COND:v[0-9]+]] = lt
    // CHECK-NEXT: coverage
    // CHECK-NEXT: jumpi [[COND]]
    // CHECK: icall @add
    function loop(uint256 n) external {
        for (uint256 i = 0; i < n; i++) {
            add(i);
        }
    }

    // CHECK-LABEL: fn @twice
    // CHECK: coverage
    // CHECK: icall @add
    // CHECK: coverage
    // CHECK: icall @add
    function twice(uint256 a) external {
        add(a);
        add(a);
    }
}
