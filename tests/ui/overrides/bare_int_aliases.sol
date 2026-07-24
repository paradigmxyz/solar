//@ check-pass
// `int` and `uint` are aliases for `int256` and `uint256`.
// Override matching must compare the canonical types.

interface IERC20Like {
    function permit(
        address owner,
        address spender,
        uint amount,
        uint deadline,
        uint8 v,
        bytes32 r,
        bytes32 s
    ) external;
}

contract ERC20Like is IERC20Like {
    function permit(
        address owner,
        address spender,
        uint256 amount,
        uint256 deadline,
        uint8 v,
        bytes32 r,
        bytes32 s
    ) public override {}
}

abstract contract SignedBase {
    function f(int value) public virtual returns (int);
}

contract SignedDerived is SignedBase {
    function f(int256 value) public pure override returns (int256) {
        return value;
    }
}
