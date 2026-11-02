//@ revisions: parsing lowering analysis
//@[parsing] compile-flags: --stop-after parsing --emit abi
//@[lowering] compile-flags: --stop-after lowering --emit abi
//@[analysis] compile-flags: --stop-after analysis --emit abi

// Each stage reports its own errors and none from later stages.

contract C {
    function f() public pure returns (uint256) {
        bool b = 1;
        //~[analysis]^ ERROR: mismatched types
        return b;
        //~[analysis]^ ERROR: mismatched types
    }

    function g() public {
        undefined_name = 1;
        //~[lowering,analysis]^ ERROR: unresolved symbol `undefined_name`
    }
}
