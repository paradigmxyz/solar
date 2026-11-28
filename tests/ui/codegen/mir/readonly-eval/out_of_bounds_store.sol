//@ codegen-matrix: standard
//@ run-call: a => 0x6666

// A store past the end of `p` writes the word `q` starts at, so the later store to `q` replaces
// it and the call must not fold to the stale value. The branches keep `read` from being inlined.
contract OutOfBoundsStore {
    function a() external pure returns (uint256) {
        bytes memory p = new bytes(0);
        bytes memory q = new bytes(0);
        assembly {
            mstore(add(p, 32), 0x1111)
            mstore(q, 0x2222)
        }
        return read(p);
    }

    function read(bytes memory data) internal pure returns (uint256 result) {
        assembly { result := mload(add(data, 32)) }
        uint256 k = result >> 200;
        if (k == 0) result *= 3;
        if (k == 1) result = result * 4 + 1;
        if (k == 2) result = result * 5 + 2;
        if (k == 3) result = result * 6 + 3;
        if (k == 4) result = result * 7 + 4;
        if (k == 5) result = result * 8 + 5;
        if (k == 6) result = result * 9 + 6;
        if (k == 7) result = result * 10 + 7;
    }
}
