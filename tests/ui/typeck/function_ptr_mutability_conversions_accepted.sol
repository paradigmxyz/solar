//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_payable_nonpayable.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_pure_nonpayable.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_pure_view.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_view_nonpayable.sol

contract PayableToNonpayable {
    function h() payable external {}
    function f() view external returns (bytes4) {
        function() external g = this.h;
        return g.selector;
    }
}

contract PureToNonpayable {
    function h() pure external {}
    function f() view external returns (bytes4) {
        function() external g = this.h;
        return g.selector;
    }
}

contract PureToView {
    function h() pure external {}
    function f() view external returns (bytes4) {
        function() view external g = this.h;
        return g.selector;
    }
}

contract ViewToNonpayable {
    int dummy;
    function h() view external { dummy; }
    function f() view external returns (bytes4) {
        function() external g = this.h;
        return g.selector;
    }
}
