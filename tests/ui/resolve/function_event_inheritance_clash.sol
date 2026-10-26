// ported-from: test/libsolidity/syntaxTests/events/function_event_inheritance_clash.sol

contract B {
    event dup();
}
contract A {
    function dup() public returns (uint) { //~ ERROR: identifier `dup` already declared
        return 1;
    }
}
contract C is B, A {
}
