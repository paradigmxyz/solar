//@ codegen-matrix: standard
//@ run-call: countLoop() => 10
//@ run-call: sumIndexed() => 31
//@ run-call: multiDecl() => 7
//@ run-call: asmUse() => 12
//@ run-call: lhsIndexMutation() => 410
//@ run-call: branchScopeLoop true, 3 => 4
//@ run-call: branchScopeLoop false, 3 => 5

// Side effects inside a variable declaration's initializer must mark the
// mutated locals as assigned: `uint256 x = xs[i++];` otherwise leaves `i` as
// an SSA constant 0, turning the loop into an infinite loop (found via
// morpho-blue's `extSloads`).

contract DeclInitializerMutation {
    function countLoop() external pure returns (uint256 n) {
        uint256 i;
        while (i < 5) {
            uint256 j = i++;
            n += j;
        }
    }

    function sumIndexed() external pure returns (uint256 s) {
        uint256[3] memory xs = [uint256(7), 11, 13];
        for (uint256 i; i < 3;) {
            uint256 x = xs[i++];
            s += x;
        }
    }

    function multiDecl() external pure returns (uint256) {
        uint256 i = 3;
        (uint256 a, uint256 b) = (i++, i);
        return a + b;
    }

    function asmUse() external pure returns (uint256 r) {
        uint256 i;
        while (i < 4) {
            uint256 j = i++;
            assembly {
                r := add(r, mul(j, 2))
            }
        }
    }

    function lhsIndexMutation() external pure returns (uint256 r) {
        uint256[] memory a = new uint256[](4);
        uint256 i;
        while (i < a.length) {
            uint256 value = i + 1;
            a[i++] = value;
        }
        r = i * 100 + a[0] + a[1] + a[2] + a[3];
    }

    function branchScopeLoop(bool condition, uint256 n) external pure returns (uint256 r) {
        if (condition) {
            uint256 branchLocal = 1;
            r = branchLocal;
        } else {
            uint256 branchLocal = 2;
            r = branchLocal;
        }
        for (uint256 i; i < n; ++i) {
            r += i;
        }
    }
}
