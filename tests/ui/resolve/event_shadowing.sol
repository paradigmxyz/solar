// Contract events that shadow file-level declarations.

import "./auxiliary/shadowed_event.sol";

event Overloaded();
event Overloaded(uint);
function freeFunction() {}
struct S {
    uint x;
}
uint constant K = 1;
error Er();
event Conflict();

contract OtherDeclarations {
    event Overloaded(); //~ WARN: this declaration shadows an existing declaration
    event freeFunction(); //~ WARN: this declaration shadows an existing declaration
    event S(); //~ WARN: this declaration shadows an existing declaration
    event K(); //~ WARN: this declaration shadows an existing declaration
    event Er(); //~ WARN: this declaration shadows an existing declaration
    event Imported(); //~ WARN: this declaration shadows an existing declaration
}

contract AlreadyDeclared {
    uint Conflict;
    event Conflict(); //~ ERROR: identifier `Conflict` already declared
}

contract Base {
    event Inherited();
}

contract Derived is Base {
    event Inherited(uint);
}
