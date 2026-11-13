//@ compile-flags: -Zcheck-analyses=true -Zmir-pipeline=none -Zdump=mir
//@ filecheck:

// With recording on, `@custom:requires` clauses become `spec_requires` records
// at function entry and `@custom:ensures` clauses `spec_ensures` records before
// each return, implied by the premises. Parameters read their values on entry,
// named return values the values returned, and a narrow signed integer is
// sign-extended to a word.
contract SpecRecords {
    // CHECK-LABEL: fn @next(
    // CHECK: [[PRE:v[0-9]+]] = lt arg0, 100
    // CHECK-NEXT: assume ne spec_requires, [[PRE]], false
    // CHECK: [[R:v[0-9]+]] = checked_add uint256, arg0, 1
    // CHECK-NEXT: [[GT:v[0-9]+]] = lt arg0, [[R]]
    // CHECK-NEXT: [[UNMET:v[0-9]+]] = eq [[PRE]], false
    // CHECK-NEXT: [[OBLIGATION:v[0-9]+]] = or [[UNMET]], [[GT]]
    // CHECK-NEXT: assume ne spec_ensures, [[OBLIGATION]], false
    // CHECK-NEXT: ret [[R]]
    /// @custom:requires x < 100
    /// @custom:ensures r > x
    function next(uint256 x) external pure returns (uint256 r) {
        r = x + 1;
        x = 0;
    }

    // CHECK-LABEL: fn @max(
    // CHECK: lt arg1, arg1
    // CHECK: assume ne spec_ensures, {{v[0-9]+}}, false
    // CHECK-NEXT: ret arg1
    // CHECK: lt arg0, arg0
    // CHECK: assume ne spec_ensures, {{v[0-9]+}}, false
    // CHECK-NEXT: ret arg0
    /// @custom:ensures r >= a && r >= b
    function max(uint256 a, uint256 b) external pure returns (uint256 r) {
        if (a > b) return a;
        return b;
    }

    // CHECK-LABEL: fn @negative(
    // CHECK: [[X:v[0-9]+]] = signextend 0, {{.+}}
    // CHECK: slt [[X]], 0
    // CHECK: assume ne spec_ensures
    /// @custom:ensures neg == (x < 0)
    function negative(int8 x) external pure returns (bool neg) {
        neg = x < 0;
    }

    // CHECK-LABEL: fn @skipped(
    // CHECK-NOT: assume
    // CHECK: ret
    /// @custom:ensures r == keccak256(x)
    //~^ WARN: specification clause skipped
    /// @custom:ensures r == y
    //~^ WARN: specification clause skipped
    /// @custom:ensures r ==
    //~^ WARN: specification clause skipped
    /// @custom:ensures r + x
    //~^ WARN: specification clause skipped
    /// @custom:ensures r && x > 0
    //~^ WARN: specification clause skipped
    function skipped(uint256 x) external pure returns (uint256 r) {
        r = x;
    }
}
