// ported-from: test/libsolidity/syntaxTests/events/file_level_event_overloading_vs_shadowing.sol

event E();

contract C {
    event E(uint); //~ WARN: this declaration shadows an existing declaration

    function f() public {
        emit E(); //~ ERROR: wrong argument count for function call: 0 arguments given but expected 1
        emit E(1);
    }
}
