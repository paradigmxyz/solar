// ported-from: test/libsolidity/syntaxTests/events/event_nested_array.sol
pragma abicoder v1;
contract c {
    event E(uint[][]); //~ ERROR: this type is only supported in ABI coder v2
}
