//@ run-call: Harness::run() => 1

// Hand-written assembly may assemble an image at fixed low addresses and read
// it back with `keccak256`/`create2`, as the ERC-6551 registry does for its
// proxy initcode at `[0x55, 0x10c)`. The backend placed its spill area at
// `0x80`, so the predicted account address (live across the `create2`
// branch) was spilled to `0xa0`, inside the image, and the deployed proxy's
// footer carried those bytes in place of the chain id. The spill base must
// sit above every constant-addressed access of the function.

contract Registry {
    function createAccount(
        address implementation,
        bytes32 salt,
        uint256 chainId,
        address tokenContract,
        uint256 tokenId
    ) external returns (address) {
        assembly {
            pop(chainId)
            calldatacopy(0x8c, 0x24, 0x80) // salt, chainId, tokenContract, tokenId
            mstore(0x6c, 0x5af43d82803e903d91602b57fd5bf3) // ERC-1167 footer
            mstore(0x5d, implementation)
            mstore(0x49, 0x3d60ad80600a3d3981f3363d3d373d3d3d363d73) // constructor + header
            mstore8(0x00, 0xff)
            mstore(0x35, keccak256(0x55, 0xb7))
            mstore(0x01, shl(96, address()))
            mstore(0x15, salt)
            let computed := keccak256(0x00, 0x55)
            if iszero(extcodesize(computed)) {
                let deployed := create2(0, 0x55, 0xb7, salt)
                if iszero(deployed) {
                    mstore(0x00, 0x20188a59) // `AccountCreationFailed()`
                    revert(0x1c, 0x04)
                }
                mstore(0x6c, deployed)
                log4(
                    0x6c,
                    0x60,
                    0x79f19b3655ee38b1ce526556b7731a20c8f218fbda4a3990b6cc4172fdf88722,
                    implementation,
                    tokenContract,
                    tokenId
                )
                return(0x6c, 0x20)
            }
            mstore(0x00, shr(96, shl(96, computed)))
            return(0x00, 0x20)
        }
    }
}

contract Harness {
    function run() external returns (uint256) {
        Registry registry = new Registry();
        address account = registry.createAccount(
            0x1111111111111111111111111111111111111111,
            bytes32(uint256(0x2200000000000000000000000000000000000000000000000000000000000033)),
            31337,
            0x4444444444444444444444444444444444444444,
            0x5555
        );
        bytes memory code = account.code;
        require(code.length == 0xad, "length");
        uint256 chainId;
        uint256 tokenContract;
        uint256 tokenId;
        assembly {
            chainId := mload(add(code, 0x6d))
            tokenContract := mload(add(code, 0x8d))
            tokenId := mload(add(code, 0xad))
        }
        require(chainId == 31337, "chain id");
        require(tokenContract == uint160(0x4444444444444444444444444444444444444444), "token contract");
        require(tokenId == 0x5555, "token id");
        return 1;
    }
}
