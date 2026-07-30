// `require(condition, SomeError(...))` accepts error constructors whose
// callee is a member path (`Lib.SomeError()`), not only plain identifiers,
// matching solc 0.8.26.

library Errors {
    error FailedCall();
    error InsufficientBalance(uint256 balance, uint256 needed);
}

error TopLevel(uint256 x);

contract C {
    error Local();

    function ok(bool cond, uint256 b) public pure {
        require(cond, Errors.FailedCall());
        require(cond, Errors.InsufficientBalance(b, 100));
        require(cond, TopLevel(b));
        require(cond, Local());
        require(cond, "plain message");
        require(cond);
    }

    function bad(bool cond) public pure {
        require(cond, Errors.FailedCall); //~ ERROR: mismatched types
        require(cond, Errors.InsufficientBalance(1)); //~ ERROR: wrong argument count for function call: 1 arguments given but expected 2
    }
}
