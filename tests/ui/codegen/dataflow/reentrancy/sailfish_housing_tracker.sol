//@ compile-flags: -Zdataflow=reentrancy
//@ filecheck:
// Sailfish, Sec. VIII-B: a housing tracker that allows more than one active listing per
// owner. The listing flag is set after the escrow call, so a reentrant `list` passes the
// check again and corrupts the tracker's data without moving Ether out.

// CHECK: finding: reentrancy single-function @list {{.*}} writes slot(0)[caller] after the call; reentrant @list reads slot(0)[caller] (stale read)
interface IEscrow {
    function hold(address seller) external payable;
}

contract HousingTracker {
    mapping(address => bool) hasListing;
    mapping(uint256 => address) listings;
    uint256 next;

    function list(IEscrow escrow) external payable {
        require(!hasListing[msg.sender]);
        //~^ NOTE: a reentrant call to `list` can read it here (stale read)
        escrow.hold{value: msg.value}(msg.sender);
        //~^ WARN: possible single-function reentrancy: `list` writes storage after an external call
        listings[next++] = msg.sender;
        hasListing[msg.sender] = true;
        //~^ NOTE: `list` writes `slot(0)[caller]` after the call
    }
}
