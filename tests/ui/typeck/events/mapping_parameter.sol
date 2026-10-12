// ported-from: test/libsolidity/syntaxTests/types/mapping/event_parameter.sol
contract C {
    event E (mapping (uint => uint) [2]); //~ ERROR: types containing mappings cannot be event parameter types
}
