//@compile-flags: -Ztypeck

contract StructConstructorNamedArgs {
    struct S {
        uint256 a;
        bool b;
    }

    function test() public pure {
        S memory s = S({b: true, a: 1});
        s.a;
    }
}
