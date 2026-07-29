//@ run-call: D::f() => 15

contract A {
    function f() public virtual returns (uint256) {
        return 1;
    }
}

contract B is A {
    function f() public virtual override returns (uint256) {
        function() internal returns (uint256) target = super.f;
        return target() | 2;
    }
}

contract C is A {
    function f() public virtual override returns (uint256) {
        function() internal returns (uint256) target = super.f;
        return target() | 4;
    }
}

contract D is B, C {
    function f() public override(B, C) returns (uint256) {
        function() internal returns (uint256) target = super.f;
        return target() | 8;
    }
}
