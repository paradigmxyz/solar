//@ codegen-matrix: standard
//@ run-call: run(uint256,bool) 1, true => 37
//@ run-call: run(uint256,bool) 1, false => 52
//@ run-call: run(uint256,bool) 2, true => 82
// Two storage fetches around a state-changing call feed a diamond on a
// struct condition; the values spilled in one arm must be stored on the
// sibling's path too.
contract SpillStoreSiblingArmsFetch {
    struct Item {
        uint256 amount;
        bool flagged;
    }

    mapping(uint256 => Item) items;
    uint256 counter;

    constructor() {
        items[1] = Item(10, true);
        items[2] = Item(20, false);
        items[3] = Item(30, true);
    }

    function fetch(uint256 key) internal view returns (uint256) {
        return items[key].amount;
    }

    function bump() internal {
        counter += 7;
    }

    function run(uint256 key, bool cond) external returns (uint256) {
        uint256 first = fetch(key);
        bump();
        uint256 second = fetch(key + 1);
        Item storage item = items[key];
        uint256 total;
        if (item.flagged && cond) {
            total = first + second;
        } else {
            total = first * 2 + second + 5;
        }
        return total + counter;
    }
}
