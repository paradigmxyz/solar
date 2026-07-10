//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_payable_pure.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_payable_view.sol

contract PayableToPure {
    function h() payable external {}
    function f() view external returns (bytes4) {
        function() pure external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}

contract PayableToView {
    function h() payable external {}
    function f() view external returns (bytes4) {
        function() view external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}
