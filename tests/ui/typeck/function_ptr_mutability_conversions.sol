//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_nonpayable_payable.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_nonpayable_pure.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_nonpayable_view.sol

contract NonpayableToPayable {
    function h() external {}
    function f() view external returns (bytes4) {
        function() payable external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}

contract NonpayableToPure {
    function h() external {}
    function f() view external returns (bytes4) {
        function() pure external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}

contract NonpayableToView {
    function h() external {}
    function f() view external returns (bytes4) {
        function() view external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}
