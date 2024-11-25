//@ compile-flags: --stop-after parsing

/// @dev func
function f(
    /// @dev arg
    uint x
) returns (
    /// @dev ret
    uint y
) {
    /// @dev statement
    SingleVaultSFData memory data = SingleVaultSFData(
        /// a
        LiqRequest(
            _buildLiqBridgeTxData(
                LiqBridgeTxDataArgs(
                    /// @dev placeholder value, not used
                    0
                )
            )
        )
    );

    /// @dev msg sender is wallet, tx origin is deployer
    SuperformRouter(payable(getContract(SOURCE_CHAIN, "SuperformRouter"))).singleXChainSingleVaultDeposit{
        value: 2 ether
    }(req);
}
