//@compile-flags: -Ztypeck
// Test: msg.value in view function (5887) - note: msg.value in nonpayable IS allowed in Solidity

contract C {
    // ERROR: msg.value in view function requires payable
    function viewMsgValue() public view returns (uint256) {
        return msg.value;
        //~^ ERROR: "msg.value" and "callvalue()" can only be used in payable public functions
    }

    // OK: msg.value in payable function
    function goodPayable() public payable returns (uint256) {
        return msg.value;
    }

    // OK: msg.value in non-payable is allowed (value will just be 0)
    function nonPayableMsgValue() public returns (uint256) {
    //~^ WARN: function state mutability can be restricted to payable
        return msg.value;
    }
}
