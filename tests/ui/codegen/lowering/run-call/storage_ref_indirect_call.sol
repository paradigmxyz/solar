//@ codegen-matrix: standard
//@ run-call: bind true => 5
//@ run-call: bind false => 4

// A storage reference returned through a function pointer binds the slot,
// like one returned by a direct call.

contract StorageRefIndirectCall {
    uint256[] first;
    uint256[] second;

    constructor() {
        first.push(11);
        second.push(22);
        second.push(33);
    }

    function getFirst() internal view returns (uint256[] storage) {
        return first;
    }

    function getSecond() internal view returns (uint256[] storage) {
        return second;
    }

    function bind(bool condition) external view returns (uint256) {
        uint256[] storage p = (condition ? getFirst : getSecond)();
        uint256[] storage q = condition ? (condition ? getSecond : getFirst)() : first;
        return p.length + 2 * q.length;
    }
}
