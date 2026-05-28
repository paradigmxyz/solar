//@ignore-host: windows
//@compile-flags: --emit=mir

// Recursive functions in Solidity. The MIR lowering inlines internal calls
// with cycle detection — recursive self-references are replaced with a
// placeholder `0` to avoid infinite inlining (see lower/mod.rs::try_enter_inline).
// This .stdout snapshot documents that behavior; if the lowering changes how
// it handles recursion, this test will diff and we'll notice.
contract Recursive {
    function fact(uint256 n) public pure returns (uint256) {
        if (n <= 1) return 1;
        return n * fact(n - 1);
    }

    function fib(uint256 n) public pure returns (uint256) {
        if (n <= 1) return n;
        return fib(n - 1) + fib(n - 2);
    }
}
