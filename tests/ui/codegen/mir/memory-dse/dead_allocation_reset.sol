//@ codegen-matrix: standard
//@ run-call: f 7 => 7
//@ run-call: neighbour 7 => 7
//@ run-call: grown 0 => 57005
//@ run-call: wrapped 7, 3618502788666131106986593281521497120414687020801267626233049500247285301246 => 7

// Rewinding the free-memory pointer puts the next object over an earlier one, and assembly can read
// an object past its end into its neighbour, so neither object's stores are dead.
contract DeadAllocationReset {
    function f(uint256 x) external pure returns (uint256 r) {
        uint256[] memory o = new uint256[](2);
        assembly {
            mstore(0x40, o)
        }
        uint256[] memory a = new uint256[](2);
        a[0] = x;
        r = o[0];
    }

    function neighbour(uint256 x) external pure returns (uint256 r) {
        uint256[1] memory a;
        uint256[1] memory b;
        b[0] = x;
        assembly ("memory-safe") {
            r := mload(add(a, 0x20))
        }
    }

    function grown(uint256 n) external pure returns (uint256 r) {
        bytes memory a = new bytes(n);
        bytes memory b = new bytes(32);
        assembly {
            mstore(add(a, 32), 0xdead)
        }
        r = b.length;
    }

    function wrapped(uint256 x, uint256 i) external pure returns (uint256) {
        uint256[] memory a = new uint256[](1);
        a[0] = x;
        uint256[] memory b = new uint256[](1);
        assembly {
            mstore(b, not(0))
        }
        return b[i];
    }
}
