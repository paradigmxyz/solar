//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime

// A value used as a `LOG` topic and then used again *later in the same block*
// (here `value` is both the event topic and the stored word) must survive the
// `LOG`, which consumes all of its operands. The value is not live-out of the
// block, so the cross-block spill machinery does not preserve it; the `LOG`
// emission itself must spill any operand that is still live afterwards. Without
// that, scheduling the second use panicked with "value is not on stack, not
// spilled" (seen on nitro-contracts InboxStub).

contract LogTopicReuse {
    event Ping(uint256 indexed value);

    uint256 public last;
    mapping(uint256 => uint256) store;

    function f(uint256 k) external {
        uint256 value = store[k];
        emit Ping(value);
        last = value;
    }
}
