// ported-from: test/libsolidity/syntaxTests/getter/nested_structs.sol
pragma abicoder v1;
contract C {
    struct Y {
        uint b;
    }
    struct X {
        Y a;
    }
    mapping(uint256 => X) public m; //~ ERROR: the following types are only supported for getters in ABI coder v2: `struct C.Y memory`
}
