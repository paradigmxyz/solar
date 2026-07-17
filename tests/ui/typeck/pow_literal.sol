// A literal base raised to a non-constant exponent is a runtime value typed
// `uint256` (matching solc), so `bytes32((2 ** (n + 1)) - 1)` type-checks (a
// bitmask idiom used by OpenZeppelin's Governor).
contract C {
    function f(uint8 n) public pure returns (bytes32 a, uint256 b) {
        a = bytes32((2 ** (n + 1)) - 1);
        b = 2 ** n;
    }
}
