//@ compile-flags: -Zdataflow=reentrancy
//@ filecheck:
// Sailfish, Fig. 1b: a destructive-write race between two `reserve` transactions. It moves
// no Ether, so, as in Sailfish, it is not a transaction-order finding, and without an
// external call it is not reentrancy either.

// CHECK: fn @reserve:
// CHECK-NOT: finding:
contract Queue {
    mapping(uint256 => address) slots;

    function reserve(uint256 slot) public {
        if (slots[slot] == address(0)) {
            slots[slot] = msg.sender;
        }
    }
}
