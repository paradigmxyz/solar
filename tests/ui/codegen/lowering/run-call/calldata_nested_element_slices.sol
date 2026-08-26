//@ codegen-matrix: standard
//@ run-call: CalldataNestedElementSlices::nestedBytes() => 5
//@ run-call: CalldataNestedElementSlices::structMember() => 3, true
//@ run-call: CalldataNestedElementSlices::tupleTernary() => 2

contract CalldataNestedElementSlices {
    struct Execution {
        address target;
        uint256 value;
        bytes callData;
    }

    function nestedBytes() external view returns (uint256) {
        bytes[] memory values = new bytes[](2);
        values[0] = hex"1122";
        values[1] = hex"334455";
        return this.sum(values);
    }

    function sum(bytes[] calldata values) external pure returns (uint256 total) {
        for (uint256 i; i < values.length; ++i) {
            bytes calldata value = values[i];
            total += _length(value);
        }
    }

    function structMember() external view returns (uint256, bool) {
        Execution[] memory values = new Execution[](1);
        values[0] = Execution(address(1), 2, hex"abcdef");
        return this.inspect(values);
    }

    function inspect(Execution[] calldata values) external pure returns (uint256 length, bool) {
        Execution calldata item = values[0];
        bytes calldata data = item.callData;
        uint256 end;
        assembly {
            end := add(data.offset, data.length)
        }
        return (data.length, end <= msg.data.length);
    }

    function _length(bytes calldata data) private pure returns (uint256) {
        return data.length;
    }

    function tupleTernary() external view returns (uint256) {
        return this.useTuple(hex"11223344", false);
    }

    function useTuple(bytes calldata data, bool empty) external pure returns (uint256) {
        (, , bytes calldata tail) = _decode(data, empty);
        return tail.length;
    }

    function _decode(bytes calldata data, bool empty)
        private
        pure
        returns (uint48 first, uint48 second, bytes calldata tail)
    {
        return empty ? (uint48(0), uint48(0), data[data.length:]) : (uint48(1), uint48(2), data[2:]);
    }
}
