// ported-from: test/libsolidity/syntaxTests/functionCalls/new_library.sol

library L {}

contract C {
    function f() public {
        new L(); //~ ERROR: cannot instantiate librarys
    }
}
