//@compile-flags: -Ztypeck
// Test: pure function cannot read msg.sender, etc.

contract C {
    function pureReadsMsgSender() public pure returns (address) {
        return msg.sender;
        //~^ ERROR: function declared as pure, but this expression (potentially) reads from the environment or state and thus requires "view"
    }

    function pureReadsMsgValue() public pure returns (uint256) {
        return msg.value;
        //~^ ERROR: "msg.value" and "callvalue()" can only be used in payable public functions
    }
}
