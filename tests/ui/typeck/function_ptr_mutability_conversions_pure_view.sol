//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_pure_payable.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_view_payable.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_view_pure.sol

contract PureToPayable {
    function h() pure external {}
    function f() view external returns (bytes4) {
        function() payable external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}

contract ViewToPayable {
    function h() view external {}
    function f() view external returns (bytes4) {
        function() payable external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}

contract ViewToPure {
    function h() view external {}
    function f() view external returns (bytes4) {
        function() pure external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}
