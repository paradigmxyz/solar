//@ compile-flags: -Ztypeck

contract A {
    function fromA() public virtual returns (uint256) {
        return 1;
    }
}

contract B {
    function fromB() public virtual returns (uint256) {
        return 2;
    }
}

contract C is A, B {
    function bothBases() public returns (uint256) {
        return super.fromA() + super.fromB();
    }
}
