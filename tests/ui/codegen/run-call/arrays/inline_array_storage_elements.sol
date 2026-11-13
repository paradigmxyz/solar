//@ codegen-matrix: standard
//@ run-call: ternary true => [11]
//@ run-call: ternary false => [22, 33]
//@ run-call: ternaryLength true => 1
//@ run-call: ternaryLength false => 2
//@ run-call: pointer true => [11]
//@ run-call: pointer false => [22, 33]
//@ run-call: stateVariable => [22, 33]

// An inline array copies each storage element to memory.

contract InlineArrayStorageElements {
    uint256[] first;
    uint256[] second;

    constructor() {
        first.push(11);
        second.push(22);
        second.push(33);
    }

    function ternary(bool flag) external view returns (uint256[] memory) {
        uint256[][1] memory arr = [flag ? first : second];
        return arr[0];
    }

    function ternaryLength(bool flag) external view returns (uint256) {
        return [flag ? first : second][0].length;
    }

    function pointer(bool flag) external view returns (uint256[] memory) {
        uint256[] storage p = flag ? first : second;
        uint256[][1] memory arr = [p];
        return arr[0];
    }

    function stateVariable() external view returns (uint256[] memory) {
        return [second][0];
    }
}
