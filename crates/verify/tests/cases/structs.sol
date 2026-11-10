pragma solidity ^0.8.20;
type Id is bytes32;
struct Market { uint128 totalSupply; uint128 totalBorrow; uint128 lastUpdate; }
struct Position { uint256 shares; uint128 collateral; }
library MathLib {
    /// @custom:ensures result == x * y / d
    function mulDivDown(uint256 x, uint256 y, uint256 d) internal pure returns (uint256) { return (x * y) / d; }
    function bump(Market storage m, uint128 by) internal { m.totalSupply += by; }
}
library IdLib {
    function raw(Id id) internal pure returns (bytes32) { return Id.unwrap(id); }
}
contract Structs {
    using MathLib for uint256;
    mapping(bytes32 => Market) public market;
    mapping(bytes32 => mapping(address => Position)) public position;
    uint256 public other;

    /// @custom:ensures market[id].totalSupply == old(market[id].totalSupply) + a
    /// @custom:ensures forall bytes32 j :: j != id ==> market[j].totalSupply == old(market[j].totalSupply)
    function ok_storage_ptr(bytes32 id, uint128 a) external {
        Market storage m = market[id];
        m.totalSupply += a;
    }

    /// @custom:ensures other == old(other)
    function ok_ptr_no_alias_other(bytes32 id) external { Market storage m = market[id]; m.lastUpdate = 5; }

    /// @custom:ensures market[id].totalSupply == old(market[id].totalSupply)
    function bad_ptr_write(bytes32 id) external { Market storage m = market[id]; m.totalSupply += 1; }

    /// @custom:ensures market[id].totalSupply == old(market[id].totalSupply) + by
    function ok_storage_param(bytes32 id, uint128 by) external { MathLib.bump(market[id], by); }

    /// @custom:ensures market[id].totalSupply == old(market[id].totalSupply)
    function bad_storage_param(bytes32 id, uint128 by) external { require(by > 0); MathLib.bump(market[id], by); }

    /// @custom:ensures market[id].totalSupply == old(market[id].totalSupply) + by
    function ok_using_storage(bytes32 id, uint128 by) external { MathLib.bump(market[id], by); }

    /// @custom:ensures result == a * b / c
    function ok_using(uint256 a, uint256 b, uint256 c) external pure returns (uint256) { return a.mulDivDown(b, c); }

    /// @custom:ensures result.collateral == c && result.shares == s
    function ok_ctor(uint256 s, uint128 c) external pure returns (Position memory) {
        return Position({collateral: c, shares: s});
    }

    /// @custom:ensures position[id][msg.sender].shares == 0 && position[id][msg.sender].collateral == 0
    function ok_delete(bytes32 id) external { delete position[id][msg.sender]; }

    /// @custom:ensures result == Id.unwrap(id)
    function ok_udvt(Id id) external pure returns (bytes32) { return IdLib.raw(id); }
}
