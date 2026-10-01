//@ compile-flags: -Zdataflow=reentrancy
//@ filecheck:
// Sailfish, Fig. 18: a transaction-order false positive shared by Sailfish and Securify.
// Only the developer can withdraw, so front-running `pay` cannot redirect or reduce what an
// attacker receives; owner-only transfers are not reported.

// CHECK: fn @withdrawDonations:
// CHECK: exit: {{.*}}requires caller == entry(slot(0))
// CHECK-NOT: finding:
contract Depay {
    address developer;
    uint256 donations;

    constructor() { developer = msg.sender; }

    function pay(uint256 donation) public payable {
        donations += donation;
    }

    function withdrawDonations(address payable recipient) public {
        require(msg.sender == developer);
        recipient.transfer(donations);
    }
}
