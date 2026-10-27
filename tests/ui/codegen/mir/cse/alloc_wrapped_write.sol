//@ codegen-matrix: standard
//@ run-call: wrapped 7, 9, 115792089237316195423570985008687907853269984665640564039457584007913129639934 => 9

// A runtime index wraps, so a write through the second array may land on the first one.
contract AllocWrappedWrite {
    function wrapped(uint256 x, uint256 y, uint256 i) external pure returns (uint256) {
        uint256[] memory a = new uint256[](1);
        a[0] = x;
        uint256 first = a[0];
        uint256[] memory b = new uint256[](1);
        assembly {
            mstore(b, not(0))
        }
        b[i] = y;
        return a[0] + first - x;
    }
}
