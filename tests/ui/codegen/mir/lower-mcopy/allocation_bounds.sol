//@ codegen-matrix: standard
//@ compile-flags: --evm-version paris
//@ run-call: copy 0 => 11, 33
//@ run-call: copy 1 => 11, 33
//@ run-call: copy 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 11, 33

contract CopyAllocationBounds {
    function copy(uint256 x) external pure returns (uint256 first, uint256 last) {
        bytes memory source = new bytes(32);
        assembly {
            source := add(x, sub(source, x))
            mstore(source, 96)
            mstore(add(source, 32), 11)
            mstore(add(source, 64), 22)
            mstore(add(source, 96), 33)
        }
        bytes memory output = abi.encodePacked(source);
        assembly {
            first := mload(add(output, 32))
            last := mload(add(output, 96))
        }
    }
}
