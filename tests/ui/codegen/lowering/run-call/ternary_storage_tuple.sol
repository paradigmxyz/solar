//@ codegen-matrix: standard
//@ run-call: returnCall true => [11], [22, 33]
//@ run-call: returnCall false => [], []
//@ run-call: returnSwap true => [11], [22, 33]
//@ run-call: returnSwap false => [22, 33], [11]
//@ run-call: returnPointer true => [11]
//@ run-call: returnPointer false => []
//@ run-call: declarePointers true => 5
//@ run-call: declarePointers false => 4
//@ run-call: declareMemory true => [11], [22, 33]
//@ run-call: declareMemory false => [22, 33], [11]
//@ run-call: assignMemory true => [11], [22, 33]
//@ run-call: assignMemory false => [], []
//@ run-call: assignPointers true => 5
//@ run-call: assignPointers false => 4
//@ run-call: assignStorage true => [11], [11]
//@ run-call: assignStorage false => [11], [22, 33]
//@ run-call: returnIndirect true => [11], [22, 33]
//@ run-call: returnIndirect false => [22, 33], [11]
//@ run-call: returnIndirectOne true => [11]
//@ run-call: returnIndirectOne false => []
//@ run-call: assignMixed true => [22, 33], [22, 33]
//@ run-call: assignMixed false => [22, 33], [11]

contract TernaryStorageTuple {
    uint256[] first;
    uint256[] second;

    constructor() {
        first.push(11);
        second.push(22);
        second.push(33);
    }

    function empty() external pure returns (uint256[] memory, uint256[] memory) {}

    function emptyOne() external pure returns (uint256[] memory) {}

    function returnCall(bool condition)
        external
        view
        returns (uint256[] memory, uint256[] memory)
    {
        return condition ? (first, second) : this.empty();
    }

    function returnSwap(bool condition)
        external
        view
        returns (uint256[] memory, uint256[] memory)
    {
        return condition ? (first, second) : (second, first);
    }

    function returnPointer(bool condition) external view returns (uint256[] memory) {
        uint256[] storage p = first;
        return condition ? p : this.emptyOne();
    }

    function declarePointers(bool condition) external view returns (uint256) {
        (uint256[] storage p, uint256[] storage q) = condition ? (first, second) : (second, first);
        return p.length + 2 * q.length;
    }

    function declareMemory(bool condition)
        external
        view
        returns (uint256[] memory, uint256[] memory)
    {
        (uint256[] memory m, uint256[] memory n) = condition ? (first, second) : (second, first);
        return (m, n);
    }

    function assignMemory(bool condition)
        external
        view
        returns (uint256[] memory m, uint256[] memory n)
    {
        (m, n) = condition ? (first, second) : this.empty();
    }

    function assignPointers(bool condition) external view returns (uint256) {
        uint256[] storage p = first;
        uint256[] storage q = first;
        (p, q) = condition ? (first, second) : (second, first);
        return p.length + 2 * q.length;
    }

    function assignStorage(bool condition) external returns (uint256[] memory, uint256[] memory) {
        (first, second) = condition ? (second, first) : (first, second);
        return (first, second);
    }

    function getFirst() internal view returns (uint256[] storage) {
        return first;
    }

    function getSecond() internal view returns (uint256[] storage) {
        return second;
    }

    function returnIndirect(bool condition)
        external
        view
        returns (uint256[] memory, uint256[] memory)
    {
        uint256[] storage p = second;
        return condition
            ? ((condition ? getFirst : getSecond)(), p)
            : (p, (condition ? getSecond : getFirst)());
    }

    function returnIndirectOne(bool condition) external view returns (uint256[] memory) {
        return condition ? (condition ? getFirst : getSecond)() : this.emptyOne();
    }

    function assignMixed(bool condition) external returns (uint256[] memory, uint256[] memory) {
        uint256[] memory m;
        // Like the via-IR pipeline, copy to memory when the assignment commits, after `first`
        // changes.
        (m, first) = condition ? (first, second) : (second, first);
        return (m, first);
    }
}
