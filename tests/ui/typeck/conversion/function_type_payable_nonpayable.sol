//@compile-flags: -Ztypeck
// Solar rejects payable -> nonpayable conversion
contract C {
    function h() payable external {
    }
    function f() view external returns (bytes4) {
        function () external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}
