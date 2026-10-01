// `@custom:solar-inline` keeps a short storage array in its own slot. Its
// elements must be narrower than a word, and it may only be indexed, measured,
// pushed, popped, deleted and copied into memory: any use that would read it
// as a storage reference, or assign it as a whole, is rejected.
library Lists {
    function sum(uint64[] storage list) internal view returns (uint256 total) {
        for (uint256 i; i < list.length; ++i) total += list[i];
    }
}

contract Inline {
    using Lists for uint64[];

    /// @custom:solar-inline
    uint64[] list;
    uint64[] other;

    /// @custom:solar-inline
    //~^ ERROR: the elements of an inline array must be value types narrower than a word
    uint256[] words;

    /// @custom:solar-inline
    //~^ ERROR: `@custom:solar-inline` must document a dynamic storage array state variable
    uint64[3] fixedList;

    function bind() internal view returns (uint256) {
        uint64[] storage r = list; //~ ERROR: an inline array cannot be a storage reference
        return r.length;
    }

    function rebind() internal {
        uint64[] storage r = other;
        r = list; //~ ERROR: an inline array cannot be a storage reference
        r.push(1);
    }

    function first(uint64[] storage l) internal view returns (uint64) {
        return l[0];
    }

    function pass() external view returns (uint64) {
        return first(list); //~ ERROR: an inline array cannot be a storage reference
    }

    function named() external view returns (uint64) {
        return first({l: list}); //~ ERROR: an inline array cannot be a storage reference
    }

    function give() internal view returns (uint64[] storage) {
        return list; //~ ERROR: an inline array cannot be a storage reference
    }

    function attached() external view returns (uint256) {
        return list.sum(); //~ ERROR: an inline array has no member `sum` this compiler keeps
    }

    function chosen(bool c) external view returns (uint64[] memory) {
        return c ? list : other; //~ ERROR: an inline array cannot be chosen by a conditional expression
    }

    function assigned(uint64[] memory values) external {
        list = values; //~ ERROR: an inline array cannot be assigned as a whole
    }

    function slot() external pure returns (uint256 s) {
        assembly {
            s := list.slot //~ ERROR: inline assembly cannot take the `.slot` of an inline array
        }
    }

    // Everything else reads and writes through the tag.
    function allowed(uint64 x) external returns (uint64[] memory, uint256, bytes32) {
        list.push(x);
        list.push();
        list[0] += 1;
        delete list[1];
        list.pop();
        other = list;
        uint64[] memory copy = list;
        bytes32 hash = keccak256(abi.encode(list));
        if (list.length > 5) delete list;
        return (copy, list.length, hash);
    }
}
