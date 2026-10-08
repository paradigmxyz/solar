//@ codegen-matrix: standard
//@ run-call: fill() => [8, 8, 9, 9, 9, 7]
//@ run-call: remove 0, 0x0000000000000000000000000000000000000001 => 23450
//@ run-call: remove 0, 0x0000000000000000000000000000000000000003 => 12450
//@ run-call: remove 0, 0x0000000000000000000000000000000000000005 => 12340
//@ run-call: remove 0, 0x0000000000000000000000000000000000000009 => 12345
pragma solidity ^0.8.0;

// Each loop's counter starts where the previous loop's stopped. Once a loop's
// pointer takes over its exit test, the counter is rebuilt from the pointer on
// the way into the next loop, whose pointer must start from that rebuilt value.
contract ContinuedCounter {
    struct Entry {
        address addr;
        uint256 amount;
    }

    mapping(uint256 => Entry[5]) lists;

    // A heap pointer leaves each loop below its end.
    function fill() public pure returns (uint256[] memory a) {
        a = new uint256[](6);
        uint256 i;
        for (; i < 2; i++) {
            a[i] = 8;
        }
        for (; i < 5; i++) {
            a[i] = 9;
        }
        for (; i < 6; i++) {
            a[i] = 7;
        }
    }

    // A storage pointer leaves the search on reaching its end, and the shift
    // continues from the found entry.
    function remove(uint256 key, address addr) public returns (uint256 packed) {
        Entry[5] storage list = lists[key];
        for (uint160 i; i < 5; i++) {
            list[i] = Entry(address(i + 1), i + 1);
        }
        for (uint256 i; i < 5; i++) {
            if (list[i].addr == addr) {
                for (uint256 j = i; j < 4; j++) {
                    list[j] = list[j + 1];
                }
                list[4] = Entry(address(0), 0);
                break;
            }
        }
        for (uint256 i; i < 5; i++) {
            packed = packed * 10 + list[i].amount;
        }
    }
}
