//@ run-call: grow => 2
//@ run-call: writeAppended 5, 42 => 42

// `push(value)` is rejected when the element type contains a mapping, because
// mapping entries cannot be copied. A plain `push()` stays allowed: it only
// grows the length, and the appended element's mapping is reachable at the
// slot the new element grew into.
contract StorageArrayPushMappingElement {
    struct Entry {
        uint256 marker;
        mapping(uint256 => uint256) values;
    }

    Entry[] private entries;

    function grow() external returns (uint256) {
        entries.push();
        entries.push();
        return entries.length;
    }

    function writeAppended(uint256 key, uint256 value) external returns (uint256) {
        entries.push();
        Entry storage entry = entries[entries.length - 1];
        entry.marker = 1;
        entry.values[key] = value;
        return entry.values[key];
    }
}
