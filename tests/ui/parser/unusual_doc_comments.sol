//@ compile-flags: --stop-after parsing

/// @dev func
function f(
    /// @dev arg
    uint x
) returns (
    /// @dev ret
    uint y
)
/// @dev why
{
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
    /// @dev ????
}

/// @dev contract
contract C {
    address[] public PROTOCOL_ADMINS = [
        0xd26b38a64C812403fD3F87717624C80852cD6D61,
        /// @dev ETH https://app.onchainden.com/safes/eth:0xd26b38a64c812403fd3f87717624c80852cd6d61
        0xf70A19b67ACC4169cA6136728016E04931D550ae
        /// @dev what the hell
    ]
    /// @dev sure
    ;
}
