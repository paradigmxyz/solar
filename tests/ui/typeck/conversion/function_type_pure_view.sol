//@compile-flags: -Ztypeck
// Solar rejects pure -> view conversion
contract C {
    function h() pure external {
    }
    function f() view external returns (bytes4) {
        function () view external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}
