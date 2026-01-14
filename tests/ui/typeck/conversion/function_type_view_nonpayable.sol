//@compile-flags: -Ztypeck
// Solar rejects view -> nonpayable conversion
contract C {
    int dummy;
    function h() view external {
        dummy;
    }
    function f() view external returns (bytes4) {
        function () external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}
