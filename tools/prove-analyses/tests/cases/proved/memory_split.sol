// Reads CSE forwards from stores into memory struct and array fields, past a
// free-memory-pointer bump and stores to other objects and scratch words.
contract MemorySplit {
    struct Pair {
        uint256 a;
        uint256 b;
    }

    function swap(uint256 x, uint256 y) external pure returns (uint256, uint256) {
        Pair memory p = Pair(x, y);
        Pair memory q = Pair(p.b, p.a);
        return (q.a + p.a, q.b + p.b);
    }

    function fixedArray(uint256 x, uint256 y) external pure returns (uint256) {
        uint256[3] memory values = [x, y, x ^ y];
        Pair memory p = Pair(values[2], values[0]);
        return p.a + p.b + values[1];
    }

    function bytesHeader() external pure returns (uint256, uint256, bytes2) {
        bytes memory a = hex"abcd";
        bytes memory b = a;
        delete a;
        return (a.length, b.length, bytes2(b));
    }
}
