//@ revisions: summary context
//@[summary] compile-flags: -Zdataflow=intervals
//@[context] compile-flags: -Zdataflow=intervals -Zdataflow-k=1
//@[summary] filecheck: --check-prefix=SUMMARY
//@[context] filecheck: --check-prefix=CONTEXT
// Interval facts with branch refinement, loop narrowing, and calling contexts. A passing
// `require` bounds an index, which proves the array's own bounds check; widening overshoots
// the loop bound and one descending round recovers it. With `k = 0` the helper has one
// summary over unknown arguments; with `k = 1` each call site gets its own exact result.

// SUMMARY: fn @inc:
// SUMMARY: summary: ret0=[1, max]
// SUMMARY: fn @bounded:
// SUMMARY: v1 = checked_mul u256, arg0, 2  ; [0, 198]
// SUMMARY: fn @constants:
// SUMMARY: summary: ret0=[2, max]
// SUMMARY: fn @loop:
// SUMMARY: v1 = phi [bb0: 0], [{{.*}}]  ; [0, 10]
// SUMMARY: fn @index:
// SUMMARY: v2 = lt arg0, 10  ; 1
// SUMMARY: v3 = eq v2, false  ; 0

// CONTEXT: fn @inc [@constants](1):
// CONTEXT-NEXT: bb0:
// CONTEXT-NEXT: checked_add u256, arg0, 1  ; 2
// CONTEXT: fn @inc [@constants](2):
// CONTEXT: summary: ret0=3
// CONTEXT: fn @constants:
// CONTEXT: summary: ret0=5
contract Intervals {
    uint256[10] data;

    function inc(uint256 x) internal pure returns (uint256) { return x + 1; }

    function bounded(uint256 x) external pure returns (uint256) {
        if (x < 100) {
            uint256 y = x * 2;
            return inc(y);
        }
        return 0;
    }

    function constants() external pure returns (uint256) { return inc(1) + inc(2); }

    function loop() external pure returns (uint256 sum) {
        for (uint256 i = 0; i < 10; i++) sum += i;
    }

    function index(uint256 i) external view returns (uint256) {
        require(i < 5);
        return data[i];
    }
}
