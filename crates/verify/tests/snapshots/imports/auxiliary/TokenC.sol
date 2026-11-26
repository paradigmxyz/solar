pragma solidity ^0.8.20;

/// A token whose `transfer` returns nothing.
interface IERC20 {
    function transfer(address to, uint256 amount) external;
}

library PayC {
    function pay(IERC20 t, address to, uint256 a) internal {
        t.transfer(to, a);
    }
}
