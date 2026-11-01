//@ revisions: gas none
//@ compile-flags: -Zdump=evm-ir,evm-ir-runtime-scheduled,evm-ir-runtime
//@[gas] compile-flags: -Ogas
//@[none] compile-flags: -Onone

// Each function's entry label has a `;` comment with its source signature, and an external entry
// also its selector. The comments are text only: they never reach executable IR or debug info.
// MIR prints the same note before each function, and block labels list their predecessors.

contract C {
    uint256 public total = 5;
    uint256 immutable limit;

    constructor(uint256 start) {
        total = start + fact(3);
        limit = start;
    }

    function add(uint256 a, uint256 b) external pure returns (uint256) {
        return a + b;
    }

    function go(uint256 n) external returns (uint256) {
        return bump(n) + fact(n) + limit;
    }

    function bump(uint256 x) internal returns (uint256) {
        total += x;
        return total;
    }

    function fact(uint256 n) internal pure returns (uint256) {
        if (n == 0) return 1;
        return n * fact(n - 1);
    }

    modifier tagged(string memory) {
        _;
    }

    function tag(uint256 x) external pure tagged("//  ( {") returns (uint256) {
        return x;
    }

    receive() external payable {}

    fallback() external {
        total += fact(4);
    }
}
