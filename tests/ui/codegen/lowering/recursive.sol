//@ignore-host: windows
//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

// Recursive functions. A recursive call can't be inlined (the inline path's
// cycle detector would substitute a `0` placeholder), so the public function is
// lowered both as its external ABI entry and as an internal-frame copy
// (`ensure_internal_mir_function`); the recursive self-call becomes an
// `internal_call` to that copy. Runtime-verified against solc: `fact(5)==120`,
// `fib(10)==55`.
contract Recursive {
    // CHECK-LABEL: fn @fact.1(
    // CHECK: [[NEXT:v[0-9]+]] = sub arg0, 1
    // CHECK: [[RECURSED:v[0-9]+]] = internal_call @fact.2, 1, [[NEXT]]
    // CHECK: mul arg0, [[RECURSED]]
    // CHECK-LABEL: fn @fact.2(
    // CHECK: internal_call @fact.2, 1,
    function fact(uint256 n) public pure returns (uint256) {
        if (n <= 1) return 1;
        return n * fact(n - 1);
    }

    // CHECK-LABEL: fn @fib.3(
    // CHECK: internal_call @fib.4, 1,
    // CHECK: internal_call @fib.4, 1,
    // CHECK: add
    // CHECK-LABEL: fn @fib.4(
    // CHECK: internal_call @fib.4, 1,
    // CHECK: internal_call @fib.4, 1,
    function fib(uint256 n) public pure returns (uint256) {
        if (n <= 1) return n;
        return fib(n - 1) + fib(n - 2);
    }

    // Mutual recursion also resolves: each non-simple callee is lowered as an
    // internal-frame copy, so neither partner is inlined. `isEven(10) == true`.
    // CHECK-LABEL: fn @isEven.5(
    // CHECK: internal_call @isOdd.6, 1,
    // CHECK-LABEL: fn @isOdd.6(
    // CHECK: internal_call @isEven.7, 1,
    // CHECK-LABEL: fn @isEven.7(
    // CHECK: internal_call @isOdd.6, 1,
    function isEven(uint256 n) public pure returns (bool) {
        if (n == 0) return true;
        return isOdd(n - 1);
    }

    // CHECK-LABEL: fn @isOdd.8(
    // CHECK: internal_call @isEven.7, 1,
    function isOdd(uint256 n) public pure returns (bool) {
        if (n == 0) return false;
        return isEven(n - 1);
    }
}
