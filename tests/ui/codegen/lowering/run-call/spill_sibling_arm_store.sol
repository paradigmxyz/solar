//@ codegen-matrix: standard
//@ run-call: run -41141462 => 41141462
//@ run-call: run 7 => 7
// `amount` is carried on the stack into the settle arm while the sibling arm
// (the native-currency call) stores it into a spill slot that was already
// reused for `delta`. The settle arm must keep its stack copy instead of
// reloading a slot its own path never wrote.
contract Recorder {
    uint256 public last;
    function sync(address) external {}
    function transfer(address, uint256 amount) external returns (bool) { last = amount; return true; }
    function take(address, address, uint256 amount) external { last = amount; }
    function settle() external payable {}
}

library Settler {
    function settle(address currency, Recorder manager, address payer, uint256 amount, bool burn) internal {
        if (burn) return;
        manager.sync(currency);
        if (payer != address(this)) return;
        Recorder(currency).transfer(address(manager), amount);
        manager.settle();
    }
    function take(address currency, Recorder manager, address recipient, uint256 amount, bool claims) internal {
        if (claims) return;
        manager.take(currency, recipient, amount);
    }
}

contract IntNegate {
    using Settler for address;
    int128 deltaSpecified;
    int128 deltaUnspecifiedBeforeSwap;
    int128 deltaUnspecifiedAfterSwap;
    Recorder manager = new Recorder();

    function run(int128 d) external returns (uint256) {
        deltaSpecified = d;
        address currency = address(manager);
        if (deltaSpecified != 0) _settleOrTake(currency, deltaSpecified);
        if (deltaUnspecifiedBeforeSwap != 0) _settleOrTake(currency, deltaUnspecifiedBeforeSwap);
        return manager.last();
    }

    function _settleOrTake(address currency, int128 delta) internal {
        if (delta > 0) {
            currency.take(manager, address(this), uint128(delta), false);
        } else {
            uint256 amount = uint256(-int256(delta));
            if (currency == address(0)) {
                manager.settle{value: amount}();
            } else {
                currency.settle(manager, address(this), amount, false);
            }
        }
    }
}
