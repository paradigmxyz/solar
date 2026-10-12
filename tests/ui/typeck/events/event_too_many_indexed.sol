// ported-from: test/libsolidity/syntaxTests/events/event_too_many_indexed.sol
contract c {
    event e(uint indexed a, bytes3 indexed b, bool indexed c, uint indexed d); //~ ERROR: more than 3 indexed arguments for event
}
