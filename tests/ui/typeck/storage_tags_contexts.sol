library Attached {
    function touch(uint256 self, uint64[] storage target, uint256 marker) internal view returns (uint256) {
        return self + target.length + marker;
    }
    function push(uint256[] storage self, uint256 index, uint256 value) internal { self[index] = value; }
}

contract Contexts {
    using Attached for uint256;
    using Attached for uint256[];
    /// @custom:solar-storage inline
    uint64[] values;
    /// @custom:solar-storage handle id dictionary
    struct Item { uint256 id; }
    uint256[] dictionary;
    Item item;
    uint256 initial = read(values); //~ ERROR: an inline array cannot be a storage reference
    uint256 changed = (item.id = 9); //~ ERROR: a handle field can only be set to an element of `dictionary` or to zero

    function read(uint64[] storage xs) internal view returns (uint256) { return xs.length; }
    function calls(uint256 receiver) external {
        receiver.touch({target: values, marker: 0}); //~ ERROR: an inline array cannot be a storage reference
        dictionary.push(0, 22); //~ ERROR: a handle dictionary has no member `push` this compiler keeps
    }
}

contract State {
    /// @custom:solar-storage inline
    uint64[] values;
    /// @custom:solar-storage handle id dictionary
    struct Item { uint256 id; }
    uint256[] dictionary;
}
abstract contract Base {
    constructor(uint64[] storage xs, uint256[] storage ys) { xs.push(1); ys.pop(); }
}
contract Constructor is State, Base {
    constructor() Base(values, dictionary) {}
    //~^ ERROR: an inline array cannot be a storage reference
    //~| ERROR: a handle dictionary cannot be a storage reference
}
