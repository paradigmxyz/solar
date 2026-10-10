// ported-from: test/libsolidity/syntaxTests/modifiers/use_in_invalid_context.sol

contract test {
    modifier mod() { _; }

    function f() public {
        mod  ; //~ ERROR: modifier can only be referenced in function headers
    }
}

contract C {
    uint a = m(1000); //~ ERROR: modifier can only be referenced in function headers

    modifier m(uint) { _; }

    function f() public {
        m(1); //~ ERROR: modifier can only be referenced in function headers
        function (uint) internal x = m; //~ ERROR: modifier can only be referenced in function headers
        x;
    }

    function g() public m(1) {}
}

contract D is C {
    function h() public {
        m(1); //~ ERROR: modifier can only be referenced in function headers
    }
}
