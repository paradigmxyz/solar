//@ revisions: 0_8 0_9
//@[0_9] compile-flags: --solc-version 0.9.0

contract C {
    function f() internal {}

    function g() public pure returns (bool) {
        return f == f;
        //~[0_9]^ ERROR: comparison of internal function pointers is not allowed
    }
}
