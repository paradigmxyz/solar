//@ codegen-matrix: standard
//@ run-call: Caller::strAndNum() => 5, 42
//@ run-call: Caller::numAndStr() => 42, 5
//@ run-call: Caller::twoStrings() => 5, 2
//@ run-call: Caller::mixed() => 3, 7, 1

// Multiple external-call return values containing dynamic types decode from
// the copied payload at their head offsets; the extra values travel through
// the multi-return buffer as fresh memory pointers.

contract Callee {
    function strAndNum() external pure returns (string memory, uint256) {
        return ("hello", 42);
    }

    function numAndStr() external pure returns (uint256, string memory) {
        return (42, "world");
    }

    function twoStrings() external pure returns (string memory, bytes memory) {
        return ("hello", hex"aabb");
    }

    function mixed() external pure returns (uint256[] memory, uint256, bytes memory) {
        uint256[] memory xs = new uint256[](3);
        xs[0] = 7;
        return (xs, 7, hex"cc");
    }
}

contract Caller {
    function strAndNum() public returns (uint256, uint256) {
        (string memory s, uint256 n) = new Callee().strAndNum();
        return (bytes(s).length, n);
    }

    function numAndStr() public returns (uint256, uint256) {
        (uint256 n, string memory s) = new Callee().numAndStr();
        return (n, bytes(s).length);
    }

    function twoStrings() public returns (uint256, uint256) {
        (string memory s, bytes memory b) = new Callee().twoStrings();
        return (bytes(s).length, b.length);
    }

    function mixed() public returns (uint256, uint256, uint256) {
        (uint256[] memory xs, uint256 n, bytes memory b) = new Callee().mixed();
        return (xs.length, xs[0] + (n - 7), b.length);
    }
}
