// ported-from: test/libsolidity/syntaxTests/events/event_struct.sol
pragma abicoder v1;
contract c {
    struct S { uint a ; }
    event E(S); //~ ERROR: this type is only supported in ABI coder v2
}
