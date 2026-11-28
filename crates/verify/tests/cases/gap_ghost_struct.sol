pragma solidity ^0.8.20;
// Ghost variables of a struct type, named plainly or through the interface that declares it.
interface IEscrow {
    struct Deposit {
        uint256 amount;
        address owner;
    }
}

/// @custom:ghost mapping(bytes32 => IEscrow.Deposit) deps
/// @custom:ghost IEscrow.Deposit last
contract GhostStruct {
    uint256 internal total;

    /// @custom:ensures deps[k].amount == a && deps[k].owner == msg.sender && last.amount == a
    function ok_record(bytes32 k, uint256 a) external {
        /// @custom:ghost deps[k] = IEscrow.Deposit(a, msg.sender)
        total += a;
        /// @custom:ghost last = deps[k]
        total += 0;
    }

    /// @custom:ensures deps[k].amount == a + 1
    function bad_record(bytes32 k, uint256 a) external {
        /// @custom:ghost deps[k] = IEscrow.Deposit(a, msg.sender)
        total += a;
    }
}
