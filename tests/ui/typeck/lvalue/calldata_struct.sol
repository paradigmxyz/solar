//@compile-flags: -Ztypeck

struct S {
    uint256 x;
    uint256 y;
}

struct Nested {
    S inner;
}

contract Test {
    uint256 state;
    
    function test(S calldata s) external {
        s.x = state; //~ ERROR: calldata structs are read-only
    }
    
    function testNested(Nested calldata n) external {
        n.inner.x = state; //~ ERROR: calldata structs are read-only
    }
}
