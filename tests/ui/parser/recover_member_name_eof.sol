//@ revisions: strict recover
//@[recover] compile-flags: -Zrecover-incomplete-input

struct Token { uint256 balance; }

contract C {
    function f(Token memory token) external pure {
        uint256 b = token.balance
        Token memory next //~ ERROR: expected one of
        //~[recover]^ ERROR: expected one of
        //~[recover]| ERROR: expected contract item
