//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=CSNR

// A calldata-slice named return (`returns (bytes calldata result)`) that is
// reassigned in the body — including its `.offset`/`.length` in assembly, the
// solady ERC-6492 unwrap idiom — lives in a two-word slice slot, so its parts
// read and write through the slot. Verified behaviorally against solc.

contract CalldataSliceNamedReturn {
    function unwrap(bytes calldata signature)
        internal
        pure
        returns (bytes calldata result)
    {
        result = signature;
        assembly {
            let x := calldataload(add(result.offset, sub(result.length, 0x20)))
            if x {
                result.length := sub(result.length, 0x20)
            }
        }
    }

    // CSNR-LABEL: fn @use
    // CSNR: calldataload
    // CSNR-NOT: unsupported
    function use(bytes calldata sig) external pure returns (uint256) {
        return unwrap(sig).length;
    }
}
