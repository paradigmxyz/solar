//@ revisions: parsing lowering analysis
//@[parsing] compile-flags: --stop-after parsing --emit abi,bin
//@[lowering] compile-flags: --stop-after lowering --emit abi,bin
//@[analysis] compile-flags: --stop-after analysis --emit abi,bin

// No stage emits output or runs code generation.

contract C {
    function f() public pure returns (uint256) {
        return 1;
    }
}
