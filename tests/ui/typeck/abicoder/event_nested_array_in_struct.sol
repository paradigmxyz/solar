// ported-from: test/libsolidity/syntaxTests/events/event_nested_array_in_struct.sol
pragma abicoder v1;
contract c {
	struct S { uint x; uint[][] arr; }
    event E(S); //~ ERROR: this type is only supported in ABI coder v2
}
