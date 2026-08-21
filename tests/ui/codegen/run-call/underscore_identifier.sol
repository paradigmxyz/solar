//@ run-call: _ => 88
//@ run-call: explicitCall => 88
//@ run-call: bareReference => 33
//@ run-call: localReference => 34
//@ run-call: modified => 45
contract C {
    function _() public pure returns (uint256) {
        return 88;
    }

    function explicitCall() public pure returns (uint256) {
        return _();
    }

    function bareReference() public pure returns (uint256) {
        _;
        return 33;
    }

    function localReference() public pure returns (uint256) {
        uint256 _ = 34;
        _;
        return _;
    }

    modifier passthrough() {
        _;
    }

    function modified() public pure passthrough returns (uint256) {
        return 45;
    }
}
