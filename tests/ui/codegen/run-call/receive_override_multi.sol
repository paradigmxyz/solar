//@ run-call: f => true

contract A {
    receive() external payable virtual {}
}

contract B {
    receive() external payable virtual {}
}

contract C is A, B {
    receive() external payable override(A, B) {}

    function f() external returns (bool) {
        (bool success,) = address(this).call("");
        return success;
    }
}
