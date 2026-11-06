//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: local => 3
//@ run-call: loop => 33

// An early `return` in an inner invocation leaves the outer invocation's locals and loop counter
// alone.
contract NestedSameLocal {
    uint256 x;

    modifier m(uint256 k) {
        uint256 loc = k * 3;
        if (k == 2) return;
        _;
        x = x * 100 + loc;
    }

    modifier lp(uint256 n) {
        for (uint256 i; i < n; i++) {
            if (n == 2 && i == 1) return;
            _;
        }
        x = x * 10 + n;
    }

    function local() external returns (uint256) {
        local1();
        return x;
    }

    function loop() external returns (uint256) {
        loop1();
        return x;
    }

    function local1() internal m(1) m(2) {
        x = 9;
    }

    function loop1() internal lp(3) lp(2) {
        x += 1;
    }
}
