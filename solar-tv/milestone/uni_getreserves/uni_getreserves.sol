// SPDX-License-Identifier: GPL-3.0
// Scratch ^0.8 port of Uniswap v2-core's UniswapV2Pair (Uniswap/v2-core@6a9e7c9),
// flattened, for staging solar-tv certification one entry point at a time.
// Lines between "//@ STAGE" and "//@ end" are kept from that stage on.
pragma solidity ^0.8.0;

interface IERC20 {
    function balanceOf(address owner) external view returns (uint256);
}




// SafeMath keeps the original revert strings: the bodies are unchecked so that
// its own require reports the overflow, not 0.8's Panic(0x11).
library SafeMath {
    function add(uint256 x, uint256 y) internal pure returns (uint256 z) {
        unchecked {
            require((z = x + y) >= x, "ds-math-add-overflow");
        }
    }

    function sub(uint256 x, uint256 y) internal pure returns (uint256 z) {
        unchecked {
            require((z = x - y) <= x, "ds-math-sub-underflow");
        }
    }

    function mul(uint256 x, uint256 y) internal pure returns (uint256 z) {
        unchecked {
            require(y == 0 || (z = x * y) / y == x, "ds-math-mul-overflow");
        }
    }
}

contract UniswapV2Pair {
    using SafeMath for uint256;

    uint256 internal totalSupply;
    mapping(address => uint256) internal balanceOf;
    mapping(address => mapping(address => uint256)) internal allowance;
    bytes32 internal DOMAIN_SEPARATOR;
    mapping(address => uint256) internal nonces;

    uint256 internal constant MINIMUM_LIQUIDITY = 10 ** 3;
    bytes4 private constant SELECTOR = bytes4(keccak256(bytes("transfer(address,uint256)")));

    address internal factory;
    address internal token0;
    address internal token1;

    uint112 private reserve0;
    uint112 private reserve1;
    uint32 private blockTimestampLast;

    uint256 internal price0CumulativeLast;
    uint256 internal price1CumulativeLast;
    uint256 internal kLast;

    uint256 private unlocked = 1;

    event Approval(address indexed owner, address indexed spender, uint256 value);
    event Transfer(address indexed from, address indexed to, uint256 value);
    event Mint(address indexed sender, uint256 amount0, uint256 amount1);
    event Burn(address indexed sender, uint256 amount0, uint256 amount1, address indexed to);
    event Swap(
        address indexed sender,
        uint256 amount0In,
        uint256 amount1In,
        uint256 amount0Out,
        uint256 amount1Out,
        address indexed to
    );
    event Sync(uint112 reserve0, uint112 reserve1);


    constructor() {
        factory = msg.sender;
        DOMAIN_SEPARATOR = keccak256(
            abi.encode(
                keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"),
                keccak256(bytes("Uniswap V2")),
                keccak256(bytes("1")),
                block.chainid,
                address(this)
            )
        );
    }

    function getReserves()
        public
        view
        returns (uint112 _reserve0, uint112 _reserve1, uint32 _blockTimestampLast)
    {
        _reserve0 = reserve0;
        _reserve1 = reserve1;
        _blockTimestampLast = blockTimestampLast;
    }










}
