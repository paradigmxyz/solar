//@revisions: homestead paris
//@[homestead] compile-flags: --evm-version homestead
//@[homestead] run-call: rt(bytes) 0x112233445566 => 0x67b2e3dafd05b82d7f58cb68f8bb7b735faec6b9258c143f8e10509016a61fb8, 6
//@[paris] compile-flags: --evm-version paris
//@[paris] run-call: rt(bytes) 0x112233445566 => 0x67b2e3dafd05b82d7f58cb68f8bb7b735faec6b9258c143f8e10509016a61fb8, 6

// On pre-Cancun targets there is no MCOPY; memory copy lowers to an
// overlap-safe MLOAD/MSTORE loop with an exact byte tail.

contract McopyPreCancun {
    function rt(bytes memory b) public pure returns (bytes32 hash, uint256 len) {
        bytes memory copied = abi.decode(abi.encode(b), (bytes));
        return (keccak256(copied), copied.length);
    }
}
