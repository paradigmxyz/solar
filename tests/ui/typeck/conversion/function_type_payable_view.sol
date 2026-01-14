//@compile-flags: -Ztypeck
contract C {
    function h() payable external {
    }
    function f() view external returns (bytes4) {
        function () view external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}
