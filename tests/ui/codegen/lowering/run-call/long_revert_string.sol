//@ codegen-matrix: standard
//@ compile-flags: --evm-version paris
//@ run-call-fail: fail33 "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" => Error("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
//@ run-call-fail: fail "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" => Error("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
//@ run-call-fail: fail68 "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" => Error("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")

contract LongRevertString {
    function fail33(string memory reason) external pure {
        revert(reason);
    }

    function fail(string memory reason) external pure {
        revert(reason);
    }

    function fail68(string memory reason) external pure {
        revert(reason);
    }
}
