//@ check-pass
//@compile-flags: -Zcodegen -Zdump=mir

contract CustomErrorPayloads {
    error EmptyError();
    error MyError(uint256 code, string message);

    function revert_empty() public pure {
        revert EmptyError();
    }

    function revert_args() public pure {
        revert MyError(7, "failed");
    }

    function require_empty(bool ok) public pure {
        require(ok, EmptyError());
    }

    function require_named(bool ok) public pure {
        require(ok, MyError({message: "failed", code: 7}));
    }
}
