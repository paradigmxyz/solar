//@ run-call: f => true, 0
// ported-from: test/libsolidity/semanticTests/fallback/fallback_override_multi.sol

contract A {
    fallback() external virtual {}
}

contract B {
    fallback() external virtual {}
}

contract C is B, A {
    fallback() external override(B, A) {}

    function f() external returns (bool, uint256) {
        (bool success, bytes memory returndata) = address(this).call("abc");
        return (success, returndata.length);
    }
}
