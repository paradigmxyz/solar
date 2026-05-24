//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_nonpayable_payable.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_nonpayable_pure.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_nonpayable_view.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_payable_nonpayable.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_payable_pure.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_payable_view.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_pure_nonpayable.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_pure_payable.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_pure_view.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_view_nonpayable.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_view_payable.sol
// ported-from: test/libsolidity/syntaxTests/conversion/function_type_view_pure.sol

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

contract PayableToNonpayable2 {
    function h() payable external {}
    function f() view external returns (bytes4) {
        function() external g = this.h;
        return g.selector;
    }
}

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

contract PureToNonpayable {
    function h() pure external {}
    function f() view external returns (bytes4) {
        function() external g = this.h;
        return g.selector;
    }
}

contract PureToPayable {
    function h() pure external {}
    function f() view external returns (bytes4) {
        function() payable external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}

contract PureToView2 {
    function h() pure external {}
    function f() view external returns (bytes4) {
        function() view external g = this.h;
        return g.selector;
    }
}

contract ViewToNonpayable2 {
    int dummy;
    function h() view external { dummy; }
    function f() view external returns (bytes4) {
        function() external g = this.h;
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

contract ViewToPure2 {
    function h() view external {}
    function f() view external returns (bytes4) {
        function() pure external g = this.h; //~ ERROR: mismatched types
        return g.selector;
    }
}
