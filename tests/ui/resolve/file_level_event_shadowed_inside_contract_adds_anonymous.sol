// ported-from: test/libsolidity/syntaxTests/events/file_level_event_shadowed_inside_contract_adds_anonymous.sol

event E();

contract C {
    event E() anonymous; //~ WARN: this declaration shadows an existing declaration
}

library L {
    event E() anonymous; //~ WARN: this declaration shadows an existing declaration
}

interface I {
    event E() anonymous; //~ WARN: this declaration shadows an existing declaration
}
