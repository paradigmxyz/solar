//@ run-call: Harness::run() => 1

// A calldata decompressor owns memory from zero through its dynamically
// growing output. Compiler spills in the loop eventually land inside that
// output and corrupt the bytes forwarded by the final delegatecall. Keep the
// loop-carried state on the EVM stack instead: there is no fixed memory address
// that remains safe for an unbounded output.

contract Decompressor {
    bool private entered;

    fallback() external payable {
        if (entered) {
            assembly {
                calldatacopy(0, 0, calldatasize())
                mstore(0, keccak256(0, calldatasize()))
                return(0, 32)
            }
        }
        entered = true;
        _decompress();
    }

    function _decompress() internal {
        assembly {
            if iszero(calldatasize()) { return(calldatasize(), calldatasize()) }
            let output := 0
            let selectorMask := not(3)
            for { let input := 0 } lt(input, calldatasize()) {} {
                let c := byte(0, xor(add(input, selectorMask), calldataload(input)))
                input := add(input, 1)
                if iszero(c) {
                    let d := byte(0, xor(add(input, selectorMask), calldataload(input)))
                    input := add(input, 1)
                    mstore(output, not(0))
                    if iszero(gt(d, 0x7f)) {
                        calldatacopy(output, calldatasize(), add(d, 1))
                    }
                    output := add(output, add(and(d, 0x7f), 1))
                    continue
                }
                mstore8(output, c)
                output := add(output, 1)
            }
            let success := delegatecall(gas(), address(), 0, output, codesize(), 0)
            returndatacopy(0, 0, returndatasize())
            if iszero(success) { revert(0, returndatasize()) }
            return(0, returndatasize())
        }
    }
}

contract Harness {
    function run() external returns (uint256) {
        Decompressor decompressor = new Decompressor();
        (bool success, bytes memory result) = address(decompressor).call(hex"ff80ff80");
        require(success, "call");
        require(abi.decode(result, (bytes32)) == keccak256(new bytes(256)), "output");
        return 1;
    }
}
