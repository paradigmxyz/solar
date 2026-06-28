//@ compile-flags: -Zdump=hir -Zhir-stats
pragma solidity ^0.8.30;

using {bump} for uint256;

function bump(uint256 value) pure returns (uint256) {
    return value + 1;
}

uint256 constant TOP = 7;

error TopError(uint256 value);

event TopEvent(address indexed who, uint256 value);

contract Complex {
    struct Entry {
        uint256 total;
        mapping(uint256 => uint256) skipped;
        uint256 count;
    }

    mapping(uint256 key => Entry[] values) public entries;
    uint256 public number = 1;

    event Seen(uint256 indexed value);
    error TooLarge(uint256 value);

    function run(uint256 limit) public returns (uint256 sum) {
        for (uint256 i = 0; i < limit; i++) {
            if (i == 2) continue;
            sum += i;
        }

        while (sum < 100) {
            sum++;
            if (sum > limit) break;
        }

        do {
            sum--;
        } while (sum > limit);

        sum = bump(sum);

        if (sum == TOP) {
            emit TopEvent(msg.sender, sum);
        }

        if (sum > 1000) {
            revert TopError(sum);
        }

        if (sum > 500) {
            revert TooLarge(sum);
        }

        emit Seen(sum);
    }
}
