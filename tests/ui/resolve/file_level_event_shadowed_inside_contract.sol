// ported-from: test/libsolidity/syntaxTests/events/file_level_event_shadowed_inside_contract.sol

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
