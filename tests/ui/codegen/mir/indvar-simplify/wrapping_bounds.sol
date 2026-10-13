//@ codegen-matrix: standard
//@ run-call: fill 3, 2 => 0x616100
//@ run-call: fill 3, 5 => 0x616161
//@ run-call: fill 3, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x616161
//@ run-call: fillWords 3, 0x0800000000000000000000000000000000000000000000000000000000000000 => [7, 7, 7]
//@ run-call: fillWords 3, 0x0800000000000000000000000000000000000000000000000000000000000001 => [7, 7, 7]
//@ run-call: fillFrom 3, 1, 5 => 0x006161
//@ run-call: fillFrom 3, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 5 => 0x000000
//@ run-call: fillFrom 3, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x000000
//@ run-call: sumAfterFirst 3 => 0
//@ run-call-fail: sumAfterFirst 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: sumAfterFirst 0x8000000000000000000000000000000000000000000000000000000000000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
// The object's length stops each loop long before a caller's count, and a counter can start
// anywhere, so a pointer compared in the counter's place could wrap at either end.
contract WrappingBounds {
    function fill(uint256 m, uint256 n) public pure returns (bytes memory b) {
        b = new bytes(m);
        uint256 len = b.length;
        uint256 i;
        while (i < n && i < len) {
            b[i] = 0x61;
            i++;
        }
    }

    function fillWords(uint256 m, uint256 n) public pure returns (uint256[] memory a) {
        a = new uint256[](m);
        uint256 len = a.length;
        uint256 i;
        while (i < n && i < len) {
            a[i] = 7;
            i++;
        }
    }

    function fillFrom(uint256 m, uint256 start, uint256 n) public pure returns (bytes memory b) {
        b = new bytes(m);
        uint256 len = b.length;
        uint256 i = start;
        while (i < n && i < len) {
            b[i] = 0x61;
            i++;
        }
    }

    // A fixed-size array's first word is an element, not a length: a counter
    // bounded by it walks the array until an index check fails.
    function sumAfterFirst(uint256 n) public pure returns (uint256 s) {
        uint256[4] memory a;
        a[0] = n;
        for (uint256 i; i < n; ++i) {
            s += a[i + 1];
        }
    }
}
