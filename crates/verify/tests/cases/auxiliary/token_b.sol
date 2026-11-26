pragma solidity ^0.8.20;

// Another declaration of `IERC20`, with other parameter names and members.
interface IERC20 {
    function transfer(address recipient, uint256 value) external returns (bool);
    function decimals() external view returns (uint8);
}

library PayB {
    function decimals(IERC20 t) internal view returns (uint8) {
        return t.decimals();
    }
}
