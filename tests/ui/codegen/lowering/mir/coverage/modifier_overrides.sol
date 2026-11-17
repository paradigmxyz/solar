//@compile-flags: -O none -Zinstrument-coverage -Zdump=mir
//@filecheck: --enable-var-scope

// A virtual modifier runs the override of the contract that is built, so each contract counts the
// body its code runs: `A` counts the expansion of `A.guard` with the counters 2 to 5, and `B` the
// expansion of `B.guard` with the counters 6 to 9. The function entry, 1, and the body, 10, are
// shared.
contract A {
    uint256 public total;

    modifier guard(uint256 n) virtual {
        require(n > 1);
        _;
    }

    // CHECK-LABEL: @module A
    // CHECK-LABEL: fn @f(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage 1
    // CHECK-NEXT: coverage 2
    // CHECK-NEXT: gt arg0, 1
    // CHECK-NEXT: coverage 3
    // CHECK: coverage 4
    // CHECK-NEXT: coverage 5
    // CHECK-NEXT: coverage 10
    // CHECK-LABEL: @module B
    // CHECK-LABEL: fn @f(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage 1
    // CHECK-NEXT: coverage 6
    // CHECK-NEXT: gt arg0, 2
    // CHECK-NEXT: coverage 7
    // CHECK: coverage 8
    // CHECK-NEXT: coverage 9
    // CHECK-NEXT: coverage 10
    function f(uint256 x) public guard(x) {
        total += x;
    }
}

contract B is A {
    modifier guard(uint256 n) override {
        require(n > 2);
        _;
    }
}
