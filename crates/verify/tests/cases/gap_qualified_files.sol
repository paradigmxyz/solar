pragma solidity ^0.8.20;
// Names resolve per declaration across files, as solc resolves them per scope: structs of one
// name in interfaces of two files are told apart by their qualified names, and an interface
// that two files each declare, read alike, is one interface with the members of both, as long
// as no file sees both declarations.
import {IEscrow} from "./auxiliary/escrow_v1.sol";
import {IEscrowV2} from "./auxiliary/escrow_v2.sol";
import {PayA} from "./auxiliary/token_a.sol";
import {PayB} from "./auxiliary/token_b.sol";

contract QualifiedFiles {
    /// @custom:ensures result == a
    function ok_v1(uint256 a) external pure returns (uint256 result) {
        IEscrow.Deposit memory d = IEscrow.Deposit(a);
        result = d.amount;
    }

    /// @custom:ensures result == u
    function ok_v2(uint256 a, address u) external pure returns (address result) {
        IEscrowV2.Deposit memory d = IEscrowV2.Deposit(a, u);
        result = d.owner;
    }

    /// @custom:ensures result == a + 1
    function bad_v1(uint256 a) external pure returns (uint256 result) {
        IEscrow.Deposit memory d = IEscrow.Deposit(a);
        result = d.amount;
    }

    /// @custom:ensures r == 7
    function ok_pay(address t, address to, uint256 a) external returns (uint256 r) {
        PayA.pay(IERC20(t), to, a);
        r = 7;
    }

    /// @custom:ensures r < 256
    function ok_decimals(address t) external view returns (uint256 r) {
        r = PayB.decimals(IERC20(t));
    }

    // The transfer may fail.
    /// @custom:ensures r
    function bad_pay(address t, address to, uint256 a) external returns (bool r) {
        r = PayA.pay(IERC20(t), to, a);
    }

    // The two declarations may have different interface ids.
    // expect: of an interface several files declare
    function bad_interface_id() external pure returns (bytes4 r) {
        r = PayA.id();
    }
}
