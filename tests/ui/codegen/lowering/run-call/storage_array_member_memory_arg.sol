//@ codegen-matrix: standard
//@ run-call: test => 3, 60

contract C {
    struct Heap {
        uint256[] data;
    }

    Heap private heap;

    constructor() {
        heap.data.push(10);
        heap.data.push(20);
        heap.data.push(30);
    }

    function test() external view returns (uint256 length, uint256 sum) {
        return consume(heap.data);
    }

    function consume(uint256[] memory values)
        internal
        pure
        returns (uint256 length, uint256 sum)
    {
        length = values.length;
        for (uint256 i; i < length; ++i) {
            sum += values[i];
        }
    }
}
