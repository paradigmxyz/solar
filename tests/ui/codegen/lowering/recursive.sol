//@compile-flags: -O none -Zdump=mir
//@filecheck:

// Recursive functions. A recursive call can't be inlined (the inline path's
// cycle detector would substitute a `0` placeholder), so the public function is
// lowered both as its external ABI entry and as an internal-frame copy
// (`ensure_internal_mir_function`); the recursive self-call becomes an
// `icall` to that copy. Runtime-verified against solc: `fact(5)==120`,
// `fib(10)==55`.
contract Recursive {
    // CHECK-LABEL: fn @fact(
    // CHECK: [[NEXT:v[0-9]+]] = checked_sub {{[ui][0-9]+}}, arg0, 1
    // CHECK: [[RECURSED:v[0-9]+]] = icall @fact, [[NEXT]]
    // CHECK: checked_mul {{[ui][0-9]+}}, arg0, [[RECURSED]]
    function fact(uint256 n) public pure returns (uint256) {
        if (n <= 1) return 1;
        return n * fact(n - 1);
    }

    // CHECK-LABEL: fn @fib(
    // CHECK: icall @fib,
    // CHECK: icall @fib,
    // CHECK: add
    function fib(uint256 n) public pure returns (uint256) {
        if (n <= 1) return n;
        return fib(n - 1) + fib(n - 2);
    }

    // Mutual recursion also resolves: each non-simple callee is lowered as an
    // internal-frame copy, so neither partner is inlined. `isEven(10) == true`.
    // CHECK-LABEL: fn @isEven(
    // CHECK: icall @isOdd,
    function isEven(uint256 n) public pure returns (bool) {
        if (n == 0) return true;
        return isOdd(n - 1);
    }

    // CHECK-LABEL: fn @isOdd(
    // CHECK: icall @isEven,
    function isOdd(uint256 n) public pure returns (bool) {
        if (n == 0) return false;
        return isEven(n - 1);
    }
}
