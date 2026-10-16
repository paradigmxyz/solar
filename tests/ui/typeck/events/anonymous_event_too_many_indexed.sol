// ported-from: test/libsolidity/syntaxTests/events/anonymous_event_too_many_indexed.sol
contract c {
    event e(uint indexed a, bytes3 indexed b, bool indexed c, uint indexed d, uint indexed e) anonymous; //~ ERROR: more than 4 indexed arguments for anonymous event
}
