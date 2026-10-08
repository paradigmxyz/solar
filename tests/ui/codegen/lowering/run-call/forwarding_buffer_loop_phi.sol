//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: check [] => 1
//@ run-call: check [1, 2, 3] => 1
//@ run-call-fail: check [0]
//@ run-call-fail: check [2, 0]
//@ run-call: checkBoth [1], [2, 3] => 2
//@ run-call-fail: checkBoth [1], [3, 0]

// A copy of dynamic length into the low forwarding buffer can cover every fixed spill slot, so
// the loop's counter rides the stack across it. The counter's phi still arrives through the
// copies its edges store to its slot, which the loop test reads before the copy can clobber it.
// The emitter treated it as lost there and kept a placeholder zero, so every call skipped the
// loop, as Seaport's conduit skipped the `NoContract` check of its batch transfers.
// The standard matrix's `mir` revision would snapshot the MIR without testing anything the
// runtime calls do not.

contract ForwardingLoop {
    function check(uint256[] calldata xs) external view returns (uint256) {
        _check(xs);
        return 1;
    }

    function checkBoth(uint256[] calldata xs, uint256[] calldata ys) external view returns (uint256) {
        _check(xs);
        _check(ys);
        return 2;
    }

    function _check(uint256[] calldata xs) internal view {
        assembly {
            let len := xs.length
            let head := xs.offset
            let next := head
            for { let i := 0 } lt(i, len) { i := add(i, 1) } {
                if iszero(calldataload(next)) { revert(0, 0) }
                calldatacopy(0xc4, head, calldataload(next))
                next := add(next, 0x20)
                if iszero(gas()) { revert(0, 0) }
            }
        }
    }
}
