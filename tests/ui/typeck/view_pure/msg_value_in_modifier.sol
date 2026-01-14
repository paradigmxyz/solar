//@compile-flags: -Ztypeck
// Test: msg.value in modifier of pure function

contract C {
    modifier m(uint _avail) {
        _;
    }

    function pureWithMsgValue() public pure m(msg.value) {}
    //~^ ERROR: "msg.value" and "callvalue()" can only be used in payable public functions
}
