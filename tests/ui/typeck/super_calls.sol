//@compile-flags: -Ztypeck

// ported-from: test/libsolidity/semanticTests/various/super.sol
// ported-from: test/libsolidity/semanticTests/inheritance/super_overload.sol

contract A {
    function f() public virtual returns (uint256) {
        return 1;
    }

    function overloaded(bool) public virtual returns (uint256) {
        return 2;
    }

    function overloaded(uint256) public virtual returns (uint256) {
        return 3;
    }
}

abstract contract B is A {
    function f() public virtual override returns (uint256) {
        return super.f() + 1;
    }

    function overloaded(bool value) public virtual override returns (uint256) {
        return super.overloaded(value) + 1;
    }

    function overloaded(uint256 value) public virtual override returns (uint256) {
        return super.overloaded(value) + 1;
    }
}

abstract contract C is B {
    function f() public override returns (uint256) {
        return super.f() + 1;
    }

    function overloadedFromC(bool value) public returns (uint256) {
        return super.overloaded(value);
    }

    function overloadedFromC(uint256 value) public returns (uint256) {
        return super.overloaded(value);
    }
}
