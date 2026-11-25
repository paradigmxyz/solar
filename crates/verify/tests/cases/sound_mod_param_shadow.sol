pragma solidity ^0.8.20;
// A modifier parameter named like a state variable shadows it: binding the argument, or
// `x = 7` in the modifier body, writes the parameter, never the state variable.
contract ModParamShadow {
    uint256 public x;

    modifier shadow(uint256 x) {
        x = 7;
        _;
    }

    modifier pin(uint256 x) {
        _;
    }

    /// @custom:ensures x == 7
    function bad_shadow() external shadow(1) {}

    /// @custom:ensures x == old(x)
    function ok_shadow() external shadow(1) {}

    /// @custom:ensures x == 1
    function bad_bind() external pin(1) {}

    /// @custom:ensures x == old(x)
    function ok_bind() external pin(1) {}

    function _set(uint256 x) internal pure {
        x = 7;
    }

    /// @custom:ensures x == 7
    function bad_internal_param() external {
        _set(1);
    }
}
