// ported-from: test/libsolidity/syntaxTests/errors/no_structs_in_abiv1.sol
pragma abicoder v1;
struct S {uint a;}
contract C {
    error MyError(S); //~ ERROR: this type is only supported in ABI coder v2
}
