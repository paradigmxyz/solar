//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/semanticTests/various/super_parentheses.sol

contract A {
    function f() public virtual returns (uint256) {
        return 1;
    }
}
contract B is A {
    function f() public virtual override returns (uint256) {
        return ((super).f)() | 2;
    }
}
contract C is A {
    function f() public virtual override returns (uint256) {
        return ((super).f)() | 4;
    }
}
contract D is B, C {
    function f() public override(B, C) returns (uint256) {
        return ((super).f)() | 8;
    }
}
