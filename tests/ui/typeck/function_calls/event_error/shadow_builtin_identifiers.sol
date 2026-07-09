//@ compile-flags: -Ztypeck

contract EventThis {
    event this();
    event this(uint256);

    function f() public returns (address) {
        emit this();
        emit this(1);
        return address(this);
    }
}

contract Base {
    function g() public virtual returns (uint256) {
        return 1;
    }
}

contract EventSuper is Base {
    event super();

    function f() public returns (uint256) {
        emit super();
        return super.g();
    }
}

contract ErrorThis {
    error this();

    function f() public pure {
        revert this();
    }
}

contract ErrorSuper {
    error super(uint256);

    function f() public pure {
        revert super(1);
    }
}
