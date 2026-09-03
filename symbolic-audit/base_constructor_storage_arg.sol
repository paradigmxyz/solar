// ported-from: test/libsolidity/semanticTests/types/struct_mapping_abstract_constructor_param.sol
// ported-from: test/libsolidity/semanticTests/types/array_mapping_abstract_constructor_param.sol
struct S { mapping(uint256 => uint256) m; }
abstract contract A { constructor(S storage s) { s.m[5] = 16; } }
// solc deploys and getM(1, 5) returns 16; solar's constructor is a bare `invalid`.
contract C is A {
    mapping(uint256 => S) m;
    constructor() A(m[1]) {}
    function getM(uint256 a, uint256 b) external view returns (uint256) { return m[a].m[b]; }
}
abstract contract B { constructor(mapping(uint256 => uint256)[] storage m) { m.push(); m[0][1] = 2; } }
// solc deploys and m(1, 0, 1) returns 2; solar reports "codegen rewrite does not support this storage access yet".
contract D is B {
    mapping(uint256 => mapping(uint256 => uint256)[]) public m;
    constructor() B(m[1]) {}
}
