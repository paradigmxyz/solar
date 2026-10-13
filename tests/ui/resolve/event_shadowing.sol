// ported-from: test/libsolidity/syntaxTests/events/file_level_event_shadowed_inside_contract.sol
// ported-from: test/libsolidity/syntaxTests/events/file_level_event_shadowed_inside_contract_adds_anonymous.sol
// ported-from: test/libsolidity/syntaxTests/events/file_level_event_overloading_vs_shadowing.sol

import "./auxiliary/shadowed_event.sol";

event E();

contract C {
    event E(); //~ WARN: this declaration shadows an existing declaration
}

library L {
    event E(); //~ WARN: this declaration shadows an existing declaration
}

interface I {
    event E(); //~ WARN: this declaration shadows an existing declaration
}

contract CAnonymous {
    event E() anonymous; //~ WARN: this declaration shadows an existing declaration
}

library LAnonymous {
    event E() anonymous; //~ WARN: this declaration shadows an existing declaration
}

interface IAnonymous {
    event E() anonymous; //~ WARN: this declaration shadows an existing declaration
}

contract Overloading {
    event E(uint); //~ WARN: this declaration shadows an existing declaration

    function f() public {
        emit E(); //~ ERROR: wrong argument count for function call: 0 arguments given but expected 1
        emit E(1);
    }
}

event Overloaded();
event Overloaded(uint);
function freeFunction() {}
struct S {
    uint x;
}
uint constant K = 1;
error Er();

contract OtherDeclarations {
    event Overloaded(); //~ WARN: this declaration shadows an existing declaration
    event freeFunction(); //~ WARN: this declaration shadows an existing declaration
    event S(); //~ WARN: this declaration shadows an existing declaration
    event K(); //~ WARN: this declaration shadows an existing declaration
    event Er(); //~ WARN: this declaration shadows an existing declaration
    event Imported(); //~ WARN: this declaration shadows an existing declaration
}

contract Base {
    event Inherited();
}

contract Derived is Base {
    event Inherited(uint);
}
