//@ compile-flags: -Ztypeck

// A mapping whose value is a fixed-size array: the array size expression must be
// type-checked exactly once (previously the mapping value was visited twice,
// causing an "already typechecked" ICE).
contract C {
    mapping(uint256 => uint32[8]) data;

    function get(uint256 k, uint256 i) public view returns (uint32) {
        return data[k][i];
    }
}
