//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: f 3 => 22

// A recursive function copies calldata over low memory. Its frame pointer at `0xa0` survives
// because the copy stays short, so the function compiles and returns the right result.
// https://github.com/paradigmxyz/solar/issues/1625

contract T {
    function rec(uint256 n) internal returns (uint256 r) {
        uint256 a = n * 3 + 1;
        assembly {
            calldatacopy(0x60, 0, calldatasize())
        }
        if (n == 0) return a;
        r = rec(n - 1) + a;
    }

    function f(uint256 n) external returns (uint256) {
        return rec(n);
    }
}
