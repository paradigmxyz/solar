//@ revisions: osaka osakaGas homestead
//@[osaka] compile-flags: -O none --evm-version osaka
//@[osakaGas] compile-flags: -O gas --evm-version osaka
//@[homestead] compile-flags: -O none --evm-version homestead
//@ run-call: TryReturnDefaultBinding::secondWord => 4, (0)

// Two return words come back over the selector-only input area and reach past it into free
// memory. The unassigned named struct return is allocated before the call, so it cannot overwrite
// the second word before the success clause decodes it.
contract TryReturnDefaultBinding {
    struct S {
        uint256 a;
    }

    function pair() external pure returns (uint256, uint256) {
        return (3, 4);
    }

    function secondWord() external view returns (uint256 r, S memory s) {
        try this.pair() returns (uint256, uint256 b) {
            r = b;
        } catch {
            r = 0;
        }
    }
}
