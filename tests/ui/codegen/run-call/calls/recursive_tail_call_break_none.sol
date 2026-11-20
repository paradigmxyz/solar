//@ compile-flags: -Onone
//@ run-call: sum 5000 => 0

contract RecursiveTailCallBreak {
    function rec(uint256 n) internal pure returns (uint256) {
        if (n == 0) return 0;
        uint256 r;
        for (;;) {
            r = rec(n - 1);
            break;
        }
        return r;
    }

    function sum(uint256 n) external pure returns (uint256) {
        return rec(n);
    }
}
