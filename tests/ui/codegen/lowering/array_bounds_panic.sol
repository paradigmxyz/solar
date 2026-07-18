//@ignore-host: windows
//@compile-flags: -Zcodegen --emit=mir

// Array indexing emits a bounds check that reverts with Panic(0x32)
// (selector 0x4e487b71, code 0x32) when `index >= length`, matching solc:
// - fixed-size arrays check against their compile-time length;
// - dynamic memory arrays and memory bytes check against the length word at
//   the base pointer;
// - storage dynamic arrays check against the length stored at the base slot;
// - calldata dynamic arrays/bytes check against the length word at
//   `4 + head`;
// - constant in-range indexes emit no check at all, and constant
//   out-of-range indexes emit an unconditional panic.
// Runtime-verified differentially against solc 0.8.30 --via-ir on anvil:
// in-range results match and out-of-range reverts are byte-identical.
contract ArrayBoundsPanic {
    uint256[] sdyn;
    uint256[3] sfix;

    function memFix(uint256 i) public pure returns (uint256) {
        uint256[3] memory x;
        x[1] = 20;
        return x[i];
    }

    function memFixConst() public pure returns (uint256) {
        uint256[3] memory x;
        x[2] = 30;
        return x[2];
    }

    function memFixConstOob() public pure returns (uint256) {
        uint256[3] memory x;
        return x[5];
    }

    function memDyn(uint256 n, uint256 i) public pure returns (uint256) {
        uint256[] memory x = new uint256[](n);
        return x[i];
    }

    function stDyn(uint256 i) public view returns (uint256) {
        return sdyn[i];
    }

    function stDynWrite(uint256 i, uint256 v) public {
        sdyn[i] = v;
    }

    function stFix(uint256 i) public view returns (uint256) {
        return sfix[i];
    }

    function cdDyn(uint256[] calldata x, uint256 i) public pure returns (uint256) {
        return x[i];
    }

    function cdFix(uint256[3] calldata x, uint256 i) public pure returns (uint256) {
        return x[i];
    }

    function cdBytes(bytes calldata b, uint256 i) public pure returns (bytes1) {
        return b[i];
    }
}
