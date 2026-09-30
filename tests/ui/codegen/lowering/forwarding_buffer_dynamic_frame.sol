//@ compile-flags: -O gas --emit=bin

// A recursive function keeps its frame pointer at `0xa0`. A copy over it cannot be protected by
// moving spills, so codegen reports it instead of emitting a broken frame.
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

//~? ERROR: codegen cannot keep the internal frame of `rec` across a dynamic low-memory write
