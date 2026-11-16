pragma solidity ^0.8.20;
// Mappings keyed by `bool`: the key is a Bool term, so the array's domain must be Bool too.
contract BoolKeys {
    mapping(bool => uint256) public byFlag;
    mapping(bool => mapping(address => uint256)) public nested;
    uint256 public x;

    /// @custom:ensures byFlag[true] == v && byFlag[false] == old(byFlag[false])
    function ok_set(uint256 v) external { byFlag[true] = v; }

    /// @custom:ensures x == old(x)
    function ok_frame() external { byFlag[false] = 5; }

    /// @custom:ensures byFlag[false] == old(byFlag[false])
    function bad_wrong_key(uint256 v) external { byFlag[false] = v; }

    /// @custom:ensures byFlag[f] == v && byFlag[!f] == old(byFlag[!f])
    function ok_symbolic_key(bool f, uint256 v) external { byFlag[f] = v; }

    /// @custom:ensures nested[f][msg.sender] == old(nested[f][msg.sender]) + 1
    function ok_nested(bool f) external { nested[f][msg.sender] += 1; }

    /// @custom:ensures nested[true][a] == old(nested[true][a])
    function bad_nested(address a) external { nested[true][a] = 1; }
}
