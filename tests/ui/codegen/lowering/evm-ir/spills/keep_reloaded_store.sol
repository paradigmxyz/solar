//@ codegen-matrix: standard
//@ run-call: probe 5 => 30
//@ run-call: probe 0 => 0

// The return-data pointer is spilled when it is defined, reloaded after the
// internal call drains the stack, and then kept resident for the block's
// remaining uses. The branch that ends the block must keep the definition-time
// store: the reload already read it, and the decode past the branch reads the
// slot again. Reduced from OpenZeppelin's `Blockhash` history-block test.
contract Target {
    function six(uint256 x) external pure returns (uint256) {
        return x * 6;
    }
}

contract KeepReloadedStore {
    Target target;

    constructor() {
        target = new Target();
    }

    // Recursive, so the inliner leaves the call in place.
    function depth(uint256 n) internal pure returns (uint256) {
        if (n == 0) return 0;
        return depth(n - 1) + 1;
    }

    function check(bool ok) internal pure {
        require(ok);
        require(depth(2) == 2);
    }

    function probe(uint256 x) external view returns (uint256) {
        (bool ok, bytes memory data) =
            address(target).staticcall(abi.encodeWithSelector(Target.six.selector, x));
        check(ok);
        return abi.decode(data, (uint256));
    }
}
