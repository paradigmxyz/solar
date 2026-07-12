// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/523_reject_interface_creation.sol

interface I {}

contract C {
    function f() public {
        new I(); //~ ERROR: cannot instantiate interfaces
    }
}
