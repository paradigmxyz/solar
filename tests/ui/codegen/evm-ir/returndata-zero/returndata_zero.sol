//@ revisions: gas size
//@ compile-flags: --evm-version paris
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ run-call: run 0 => 84
//@ run-call: run 1 => 85

contract ReturndataZero {
    uint256 stored = 42;

    function data() external pure returns (uint256, uint256, uint256) {
        return (7, 8, 9);
    }

    // The slot of `stored` is zero. Before the call `RETURNDATASIZE` can push it, but after the
    // call returned three words it holds 96, so the second read must push a literal zero.
    function run(uint256 x) external returns (uint256) {
        uint256 before = stored;
        (bool success,) = address(this).call(abi.encodeCall(this.data, ()));
        require(success);
        if (x == 0) {
            return before + stored;
        }
        return before + stored + x;
    }
}
