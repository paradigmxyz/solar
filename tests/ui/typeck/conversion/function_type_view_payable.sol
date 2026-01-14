//@compile-flags: -Ztypeck
contract C {
    function h() view external {
    }
    function f() view external returns (bytes4) {
        function () payable external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}
