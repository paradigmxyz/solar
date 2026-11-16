//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: twice => 21
//@ run-call: g 1 => 2
//@ run-call: g 2 => 1
//@ run-call: tryOk => 8
//@ run-call: tryCatch => 8

// An early `return` after the first of two `_` still leaves the enclosing invocation's parameter
// intact for its second `_`, and so does one inside a `try` block, whose variable the outer
// invocation shares.
contract NestedSamePlaceholders {
    uint256 x;

    modifier m(uint256 k) {
        _;
        if (k == 2) return;
        _;
        x = x * 10 + k;
    }

    modifier u(uint256 k) {
        try this.ext(k != 2) returns (uint256 v) {
            if (k == 3) return;
            _;
            x = x * 10 + k + v;
        } catch {
            return;
        }
    }

    function ext(bool ok) external pure returns (uint256) {
        require(ok);
        return 7;
    }

    function twice() external returns (uint256) {
        twice1();
        return x;
    }

    function g(uint256 a) external returns (uint256) {
        return g1(a);
    }

    function tryOk() external returns (uint256) {
        tryOk1();
        return x;
    }

    function tryCatch() external returns (uint256) {
        tryCatch1();
        return x;
    }

    function twice1() internal m(1) m(2) {
        x += 1;
    }

    function g1(uint256 a) internal m(a) m(2) returns (uint256 r) {
        x += 1;
        r = x;
    }

    function tryOk1() internal u(1) u(3) {
        x = 4;
    }

    function tryCatch1() internal u(1) u(2) {
        x = 4;
    }
}
