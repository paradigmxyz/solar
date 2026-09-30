//@ compile-flags: -Zdump=secir

// Covers effects inside loops, `msg.value` and `delegatecall` in loops, strict equality on
// balances, division before multiplication, modulo of block values, timestamp conditions,
// comparison constants, unchecked low-level call and `send` results, and a contract that locks
// the value it receives.

contract Hazards {
    mapping(address => uint256) credit;
    uint256 total;
    uint256 lastDraw;

    function airdrop(address payable[] calldata to) external payable {
        for (uint256 i = 0; i < to.length; i++) {
            to[i].transfer(msg.value);
            credit[to[i]] += msg.value;
        }
    }

    function batch(address target, bytes[] calldata calls) external payable {
        for (uint256 i = 0; i < calls.length; i++) {
            (bool ok,) = target.delegatecall(calls[i]);
            require(ok);
        }
    }

    function resetIfEmpty() external {
        if (address(this).balance == 0) total = 0;
    }

    function share(uint256 amount, uint256 parts, uint256 bps) external pure returns (uint256) {
        return amount / parts * bps;
    }

    function draw() external returns (uint256) {
        require(block.timestamp >= lastDraw + 1 days, "early");
        lastDraw = block.timestamp;
        return uint256(blockhash(block.number - 1)) % 10;
    }

    function magic(uint256 x, int256 y) external pure returns (bool) {
        return x == 0xdeadbeef || x > 1000 || y < -5;
    }

    function ping(address target) external {
        target.call("");
        payable(target).send(0);
    }
}

contract Piggy {
    receive() external payable {}
}
