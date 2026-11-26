pragma solidity ^0.8.20;
// A symbol imported under another name (`import {A as B}`) is that symbol.
import {LibMath as M, IEscrowV2 as Escrow} from "./auxiliary/gap_import_lib.sol";

contract ImportAlias {
    /// @custom:ensures result == 2 * x
    function ok_twice(uint256 x) external pure returns (uint256 result) {
        result = M.twice(x);
    }

    /// @custom:ensures result == x
    function bad_twice(uint256 x) external pure returns (uint256 result) {
        result = M.twice(x);
    }

    /// @custom:ensures result == u
    function ok_struct(uint256 a, address u) external pure returns (address result) {
        Escrow.Deposit memory d = Escrow.Deposit(a, u);
        result = d.owner;
    }
}
