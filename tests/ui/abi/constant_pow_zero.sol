//@ compile-flags: --emit=abi,hashes --pretty-json

contract ConstantPowZero {
    function f(uint256[(0 ** 0) + 1] calldata input)
        external pure returns (uint256[(0 ** 0) + 1] memory)
    {
        return input;
    }
}
