// SPDX-License-Identifier: MIT
pragma solidity ^0.8.36;

import {EarnVault} from "src/earn/core/EarnVault.sol";
import {HoldEngine} from "src/earn/engines/hold/HoldEngine.sol";
import {CoreMockToken, earnVaultTestNoFees} from "test/earn/helpers/EarnVaultMocks.sol";
import {EarnVaultTestDeployer} from "test/earn/helpers/EarnVaultTestDeployer.sol";

// The setUp of Earn's `test/earn/gas/HoldEngineGas.t.sol`, with fees off, behind ordinary calls.
contract TempoEarn {
    CoreMockToken immutable asset;
    CoreMockToken immutable earnShare;
    EarnVault immutable earnVault;

    constructor() {
        asset = new CoreMockToken("USD", "USD", 6);
        earnShare = new CoreMockToken("Earn USD", "eUSD", 6);
        HoldEngine engine = new HoldEngine(address(asset), address(this), "Hold USD", "hUSD");
        earnVault = EarnVaultTestDeployer.deploy(
            address(engine), address(earnShare), address(this), earnVaultTestNoFees()
        );
        engine.initializeEarnVault(address(earnVault));
        asset.mint(address(this), 1000e6);
        asset.approve(address(earnVault), type(uint256).max);
        earnShare.approve(address(earnVault), type(uint256).max);
    }

    function deposit(uint256 amount) external returns (uint256) {
        return earnVault.deposit(amount, address(this), 1);
    }

    function redeem(uint256 amount) external returns (uint256) {
        return earnVault.redeem(amount, address(this), 1);
    }

    function withdrawExact(uint256 amount) external returns (uint256) {
        return earnVault.withdrawExact(amount, address(this), type(uint256).max);
    }

    function shares() external view returns (uint256) {
        return earnShare.balanceOf(address(this));
    }

    function assets() external view returns (uint256) {
        return asset.balanceOf(address(this));
    }
}
