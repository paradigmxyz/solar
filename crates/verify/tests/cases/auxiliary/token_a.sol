pragma solidity ^0.8.20;

interface IERC20 {
    function transfer(address to, uint256 amount) external returns (bool);
    function balanceOf(address account) external view returns (uint256);
}

library PayA {
    function pay(IERC20 t, address to, uint256 a) internal returns (bool) {
        return t.transfer(to, a);
    }

    function id() internal pure returns (bytes4) {
        return type(IERC20).interfaceId;
    }
}
