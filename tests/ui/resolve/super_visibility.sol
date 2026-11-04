// ported-from: test/libsolidity/syntaxTests/super/super_in_function.sol
// ported-from: test/libsolidity/syntaxTests/super/super_in_library.sol

function freeFunction() pure {
    super; //~ ERROR: unresolved symbol `super`
}

library L {
    function libraryFunction() public {
        super; //~ ERROR: unresolved symbol `super`
    }
}

contract C {
    function contractFunction() public {
        super;
    }
}
