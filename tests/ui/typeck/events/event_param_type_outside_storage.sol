// ported-from: test/libsolidity/syntaxTests/events/event_param_type_outside_storage.sol
contract c {
    event e(uint indexed a, mapping(uint => uint) indexed b, bool indexed c, uint indexed d, uint indexed e) anonymous;
    //~^ ERROR: types containing mappings cannot be event parameter types
    //~| ERROR: more than 4 indexed arguments for anonymous event
}
