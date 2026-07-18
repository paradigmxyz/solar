//@compile-flags: -Zcodegen --emit=mir

contract AddmodMulmod {
    function am(uint x, uint y, uint n) public pure returns (uint) {
        return addmod(x, y, n);
    }

    function mm(uint x, uint y, uint n) public pure returns (uint) {
        return mulmod(x, y, n);
    }
}
