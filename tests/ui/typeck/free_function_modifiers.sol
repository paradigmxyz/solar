// ported-from: test/libsolidity/syntaxTests/freeFunctions/free_function_qualified_modifier.sol

contract C {
  modifier someModifier() { _; }
}

function fun() C.someModifier { //~ ERROR: free functions cannot have modifiers
//~^ ERROR: can only use modifiers defined in the current contract or in base contracts
}

// A file-level name in the modifier position is rejected too; solc reports that the declaration is
// neither a modifier nor a base class instead.
function fun2() fun {} //~ ERROR: free functions cannot have modifiers
//~^ ERROR: expected modifier, found function

// The restriction only applies to free functions.
contract D is C {
    function f() public someModifier {}
}
