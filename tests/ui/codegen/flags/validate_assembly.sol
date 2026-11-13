//@ revisions: gas size homestead
//@ check-pass
//@[gas] compile-flags: -Zvalidate-assembly -O gas --emit=bin,bin-runtime
//@[size] compile-flags: -Zvalidate-assembly -O size --emit=bin,bin-runtime
//@[homestead] compile-flags: -Zvalidate-assembly --evm-version homestead --emit=bin,bin-runtime
//@ normalize-stdout-test: "(?s).+" -> ""
// The assembled bytecode decodes to the assembly it was solved from: label pushes, immutable
// placeholders, library placeholders, the embedded runtime code and constructor arguments.

library Lib {
    function twice(uint256 x) external pure returns (uint256) {
        return 2 * x;
    }
}

contract C {
    uint256 public immutable seed;
    uint8 internal immutable small;

    constructor(uint256 s) {
        seed = s;
        small = uint8(s);
    }

    function pick(uint256 i) external view returns (uint256) {
        if (i == 0) return seed;
        if (i == 1) return small;
        if (i == 2) return Lib.twice(seed);
        uint256 total;
        for (uint256 j; j < i; j++) total += j;
        return total;
    }
}
