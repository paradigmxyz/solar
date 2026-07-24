//@ check-pass
//@compile-flags: -Zcodegen -Zdump=mir

contract RevertPayloads {
    function assert_panic(bool ok) public pure {
        assert(ok);
    }

    function require_message(bool ok) public pure {
        require(ok, "bad");
    }

    function revert_message() public pure {
        revert("bad");
    }
}
