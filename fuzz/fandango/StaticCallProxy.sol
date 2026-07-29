// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Test helper that preserves STATICCALL semantics during independent RPC replay.
contract FandangoStaticCallProxy {
    fallback(bytes calldata input) external returns (bytes memory) {
        require(input.length >= 20, "missing target");
        address target = address(bytes20(input[:20]));
        (bool ok, bytes memory output) = target.staticcall(input[20:]);
        assembly ("memory-safe") {
            let data := add(output, 0x20)
            let length := mload(output)
            switch ok
            case 0 { revert(data, length) }
            default { return(data, length) }
        }
    }
}
