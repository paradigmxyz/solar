// SPDX-License-Identifier: MIT
pragma solidity >=0.8.10;

import {DSTest} from "ds-test/test.sol";
import {Vm, stdError} from "forge-std/Test.sol";
import {ERC20} from "solmate/tokens/ERC20.sol";
import {MockERC20} from "solmate/test/utils/mocks/MockERC20.sol";
import {UnifapV2Pair} from "../UnifapV2Pair.sol";
import {UQ112x112} from "../libraries/UQ112x112.sol";

contract MockUser {
    function addLiquidity(
        address pair,
        address _token0,
        address _token1,
        uint256 _amount0,
        uint256 _amount1
    ) public returns (uint256) {
        ERC20(_token0).transfer(pair, _amount0);
        ERC20(_token1).transfer(pair, _amount1);

        return UnifapV2Pair(pair).mint(address(this));
    }

    function removeLiquidity(address pair, uint256 liquidity)
        public
        returns (uint256, uint256)
    {
        UnifapV2Pair(pair).transfer(pair, liquidity);

        return UnifapV2Pair(pair).burn(address(this));
    }
}

contract TestUnifapV2Pair is DSTest {
    Vm internal constant vm = Vm(HEVM_ADDRESS);

    MockERC20 public token0;
    MockERC20 public token1;
    UnifapV2Pair public pair;
    MockUser public user;

    function setUp() public {
        token0 = new MockERC20("UnifapToken0", "UT0", 18);
        token1 = new MockERC20("UnifapToken1", "UT1", 18);
        pair = new UnifapV2Pair();
        pair.initialize(address(token0), address(token1));

        token0.mint(address(this), 10 ether);
        token1.mint(address(this), 10 ether);

        user = new MockUser();
        token0.mint(address(user), 10 ether);
        token1.mint(address(user), 10 ether);
    }

    function assertBlockTimestampLast(uint256 timestamp) public {
        (, , uint32 lastBlockTimestamp) = pair.getReserves();

        assertEq(timestamp, lastBlockTimestamp);
    }

    function getCurrentMarginalPrices()
        public
        view
        returns (uint256 price0, uint256 price1)
    {
        (uint112 reserve0, uint112 reserve1, ) = pair.getReserves();

        price0 = reserve0 > 0
            ? uint256(UQ112x112.encode(reserve1)) / reserve0
            : 0;
        price1 = reserve1 > 0
            ? uint256(UQ112x112.encode(reserve0)) / reserve1
            : 0;
    }

    function assertCumulativePrices(uint256 price0, uint256 price1) public {
        assertEq(
            price0,
            pair.price0CumulativeLast(),
            "unexpected cumulative price 0"
        );
        assertEq(
            price1,
            pair.price1CumulativeLast(),
            "unexpected cumulative price 1"
        );
    }

    function assertPairReserves(uint256 _reserve0, uint256 _reserve1) public {
        assertEq(pair.reserve0(), _reserve0);
        assertEq(pair.reserve1(), _reserve1);
    }

    function testMintNewPair() public {
        token0.transfer(address(pair), 1 ether);
        token1.transfer(address(pair), 1 ether);

        uint256 liquidity = pair.mint(address(this));

        assertPairReserves(1 ether, 1 ether);
        assertEq(pair.balanceOf(address(0)), pair.MINIMUM_LIQUIDITY());
        assertEq(pair.balanceOf(address(this)), liquidity);
        assertEq(pair.totalSupply(), liquidity + pair.MINIMUM_LIQUIDITY());
    }

    function testMintWithReserve() public {
        token0.transfer(address(pair), 1 ether);
        token1.transfer(address(pair), 1 ether);
        uint256 l1 = pair.mint(address(this));

        token0.transfer(address(pair), 2 ether);
        token1.transfer(address(pair), 2 ether);
        uint256 l2 = pair.mint(address(this));

        assertPairReserves(3 ether, 3 ether);
        assertEq(pair.balanceOf(address(this)), l1 + l2);
        assertEq(pair.totalSupply(), l1 + l2 + pair.MINIMUM_LIQUIDITY());
    }

    function testMintUnequalBalance() public {
        token0.transfer(address(pair), 1 ether);
        token1.transfer(address(pair), 1 ether);
        uint256 l1 = pair.mint(address(this));

        token0.transfer(address(pair), 4 ether);
        token1.transfer(address(pair), 1 ether);
        uint256 l2 = pair.mint(address(this));

        assertPairReserves(5 ether, 2 ether);
        assertEq(pair.balanceOf(address(this)), l1 + l2);
        assertEq(pair.totalSupply(), l1 + l2 + pair.MINIMUM_LIQUIDITY());
    }

    function testMintArithmeticUnderflow() public {
        // 0x11: Arithmetic over/underflow
        vm.expectRevert(stdError.arithmeticError);

        pair.mint(address(this));
    }

    function testMintInsufficientLiquidity() public {
        token0.transfer(address(pair), 1000);
        token1.transfer(address(pair), 1000);

        vm.expectRevert(
            abi.encodeWithSignature("InsufficientLiquidityMinted()")
        );
        pair.mint(address(this));
    }

    function testMintMultipleUsers() public {
        token0.transfer(address(pair), 1 ether);
        token1.transfer(address(pair), 1 ether);
        uint256 l1 = pair.mint(address(this));

        uint256 l2 = user.addLiquidity(
            address(pair),
            address(token0),
            address(token1),
            2 ether,
            3 ether
        );

        assertPairReserves(3 ether, 4 ether);
        assertEq(pair.balanceOf(address(this)), l1);
        assertEq(pair.balanceOf(address(user)), l2);
        assertEq(pair.totalSupply(), l1 + l2 + pair.MINIMUM_LIQUIDITY());
    }

    function testBurn() public {
        token0.transfer(address(pair), 1 ether);
        token1.transfer(address(pair), 1 ether);
        uint256 liquidity = pair.mint(address(this));

        pair.transfer(address(pair), liquidity);
        pair.burn(address(this));

        assertEq(pair.balanceOf(address(this)), 0);
        assertEq(pair.totalSupply(), pair.MINIMUM_LIQUIDITY());
        assertPairReserves(pair.MINIMUM_LIQUIDITY(), pair.MINIMUM_LIQUIDITY());
        assertEq(
            token0.balanceOf(address(this)),
            10 ether - pair.MINIMUM_LIQUIDITY()
        );
        assertEq(
            token1.balanceOf(address(this)),
            10 ether - pair.MINIMUM_LIQUIDITY()
        );
    }

    function testBurnUnequal() public {
        token0.transfer(address(pair), 1 ether);
        token1.transfer(address(pair), 1 ether);
        uint256 l0 = pair.mint(address(this));

        token0.transfer(address(pair), 1 ether);
        token1.transfer(address(pair), 2 ether);
        uint256 l1 = pair.mint(address(this));

        pair.transfer(address(pair), l0 + l1);
        (uint256 amount0, uint256 amount1) = pair.burn(address(this));

        assertEq(pair.balanceOf(address(this)), 0);
        assertEq(pair.totalSupply(), pair.MINIMUM_LIQUIDITY());
        assertEq(token0.balanceOf(address(this)), 10 ether - 2 ether + amount0);
        assertEq(token1.balanceOf(address(this)), 10 ether - 3 ether + amount1);
    }

    function testBurnNoLiquidity() public {
        // 0x12: divide/modulo by zero
        vm.expectRevert(stdError.divisionError);

        pair.burn(address(this));
    }

    function testBurnInsufficientLiquidityBurned() public {
        token0.transfer(address(pair), 1 ether);
        token1.transfer(address(pair), 1 ether);
        pair.mint(address(this));

        vm.expectRevert(
            abi.encodeWithSignature("InsufficientLiquidityBurned()")
        );
        pair.burn(address(this));
    }

    function testBurnMultipleUsers() public {
        token0.transfer(address(pair), 1 ether);
        token1.transfer(address(pair), 1 ether);
        uint256 l1 = pair.mint(address(this));

        uint256 l2 = user.addLiquidity(
            address(pair),
            address(token0),
            address(token1),
            2 ether,
            3 ether
        );

        pair.transfer(address(pair), l1);
        (uint256 amount0, uint256 amount1) = pair.burn(address(this));

        assertEq(pair.balanceOf(address(this)), 0);
        assertEq(pair.balanceOf(address(user)), l2);
        assertEq(pair.totalSupply(), l2 + pair.MINIMUM_LIQUIDITY());
        assertPairReserves(3 ether - amount0, 4 ether - amount1);
        assertEq(token0.balanceOf(address(this)), 10 ether - 1 ether + amount0);
        assertEq(token1.balanceOf(address(this)), 10 ether - 1 ether + amount1);
    }

    function testBurnUnbalancedMultipleUsers() public {
        token0.transfer(address(pair), 1 ether);
        token1.transfer(address(pair), 1 ether);
        uint256 l1 = pair.mint(address(this));

        uint256 l2 = user.addLiquidity(
            address(pair),
            address(token0),
            address(token1),
            2 ether,
            3 ether
        );

        (uint256 a00, uint256 a01) = user.removeLiquidity(address(pair), l2);

        pair.transfer(address(pair), l1);
        (uint256 a10, uint256 a11) = pair.burn(address(this));

        assertEq(pair.balanceOf(address(this)), 0);
        assertEq(pair.balanceOf(address(user)), 0);
        assertEq(pair.totalSupply(), pair.MINIMUM_LIQUIDITY());

        // second user penalised for unbalanced liquidity, hence reserves unbalanced
        assertPairReserves(
            pair.MINIMUM_LIQUIDITY(),
            1 ether + pair.MINIMUM_LIQUIDITY()
        );
        assertEq(token0.balanceOf(address(this)), 10 ether - 1 ether + a10);
        assertEq(token1.balanceOf(address(this)), 10 ether - 1 ether + a11);
        assertEq(token0.balanceOf(address(user)), 10 ether - 2 ether + a00);
        assertEq(token1.balanceOf(address(user)), 10 ether - 3 ether + a01);
    }

    function testSwap() public {
        token0.transfer(address(pair), 1 ether);
        token1.transfer(address(pair), 1 ether);
        pair.mint(address(this));

        // transfer to maintain K
        token1.transfer(address(pair), 1 ether);

        pair.swap(0.5 ether, 0 ether, address(user));

        assertPairReserves(0.5 ether, 2 ether);
        assertEq(token0.balanceOf(address(user)), 10 ether + 0.5 ether);
    }

    function testSwapMultipleUserLiquidity() public {
        token0.transfer(address(pair), 1 ether);
        token1.transfer(address(pair), 1 ether);
        pair.mint(address(this));

        user.addLiquidity(
            address(pair),
            address(token0),
            address(token1),
            2 ether,
            3 ether
        );

        // transfer to maintain K
        token0.transfer(address(pair), 1 ether);

        pair.swap(0 ether, 1 ether, address(user));

        assertPairReserves(4 ether, 3 ether);
    }

    function testSwapUnderpriced() public {
        token0.transfer(address(pair), 1 ether);
        token1.transfer(address(pair), 1 ether);
        pair.mint(address(this));

        // transfer to maintain K
        token1.transfer(address(pair), 1 ether);
        pair.swap(0.4 ether, 0, address(user));

        assertPairReserves(0.6 ether, 2 ether);
    }

    function testSwapInvalidAmount() public {
        vm.expectRevert(abi.encodeWithSignature("InvalidAmount()"));
        pair.swap(0 ether, 0 ether, address(user));
    }

    function testSwapInsufficientLiquidity() public {
        token0.transfer(address(pair), 1 ether);
        token1.transfer(address(pair), 1 ether);
        pair.mint(address(this));

        vm.expectRevert(abi.encodeWithSignature("InsufficientLiquidity()"));
        pair.swap(3 ether, 0 ether, address(user));

        vm.expectRevert(abi.encodeWithSignature("InsufficientLiquidity()"));
        pair.swap(0 ether, 3 ether, address(user));
    }

    function testSwapSwapToSelf() public {
        token0.transfer(address(pair), 1 ether);
        token1.transfer(address(pair), 1 ether);
        pair.mint(address(this));

        vm.expectRevert(abi.encodeWithSignature("SwapToSelf()"));
        pair.swap(1 ether, 0 ether, address(token0));

        vm.expectRevert(abi.encodeWithSignature("SwapToSelf()"));
        pair.swap(0 ether, 1 ether, address(token1));
    }

    function testSwapInvalidConstantProductFormula() public {
        token0.transfer(address(pair), 1 ether);
        token1.transfer(address(pair), 1 ether);
        pair.mint(address(this));

        vm.expectRevert(
            abi.encodeWithSignature("InvalidConstantProductFormula()")
        );
        pair.swap(1 ether, 0 ether, address(user));

        vm.expectRevert(
            abi.encodeWithSignature("InvalidConstantProductFormula()")
        );
        pair.swap(0 ether, 1 ether, address(user));
    }

    function testCumulativePrices() public {
        vm.warp(0);
        token0.transfer(address(pair), 1 ether);
        token1.transfer(address(pair), 1 ether);
        pair.mint(address(this));

        pair.sync();
        assertCumulativePrices(0, 0);

        (
            uint256 currentPrice0,
            uint256 currentPrice1
        ) = getCurrentMarginalPrices();

        vm.warp(1);
        pair.sync();
        assertBlockTimestampLast(1);
        assertCumulativePrices(currentPrice0, currentPrice1);

        vm.warp(2);
        pair.sync();
        assertBlockTimestampLast(2);
        assertCumulativePrices(currentPrice0 * 2, currentPrice1 * 2);

        vm.warp(3);
        pair.sync();
        assertBlockTimestampLast(3);
        assertCumulativePrices(currentPrice0 * 3, currentPrice1 * 3);

        user.addLiquidity(
            address(pair),
            address(token0),
            address(token1),
            2 ether,
            3 ether
        );

        (uint256 newPrice0, uint256 newPrice1) = getCurrentMarginalPrices();

        vm.warp(4);
        pair.sync();
        assertBlockTimestampLast(4);
        assertCumulativePrices(
            currentPrice0 * 3 + newPrice0,
            currentPrice1 * 3 + newPrice1
        );

        vm.warp(5);
        pair.sync();
        assertBlockTimestampLast(5);
        assertCumulativePrices(
            currentPrice0 * 3 + newPrice0 * 2,
            currentPrice1 * 3 + newPrice1 * 2
        );

        vm.warp(6);
        pair.sync();
        assertBlockTimestampLast(6);
        assertCumulativePrices(
            currentPrice0 * 3 + newPrice0 * 3,
            currentPrice1 * 3 + newPrice1 * 3
        );
    }
}
