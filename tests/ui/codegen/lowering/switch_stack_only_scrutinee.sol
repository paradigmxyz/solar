//@ compile-flags: -Ogas
//@ run-call: outer(uint256,uint256) 0, 0 => 2
//@ run-call: outer(uint256,uint256) 3, 5 => 30
//@ run-call: outer(uint256,uint256) 0, 7 => 0

// A shared static helper keeps its arguments stack-resident with no memory
// home. Nested switches consume them as scrutinees: the inner switch reaches
// the drain-and-redispatch path, which must keep the stack-only scrutinee on
// the stack rather than reloading it from an uninitialized frame slot.
contract SwitchStackOnlyScrutinee {
    function outer(uint256 x, uint256 n) public pure returns (uint256) {
        return helper(x, n) + helper(n, x);
    }

    function helper(uint256 x, uint256 n) internal pure returns (uint256 z) {
        assembly {
            switch x
            case 0 {
                switch n
                case 0 { z := 1 }
                default { z := 0 }
            }
            default { z := mul(x, n) }
        }
    }
}
