//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: ConstructorFunctionPointerDispatch::getName => 0x646566000000
// ported-from: test/libsolidity/semanticTests/constructor/functions_called_by_constructor_through_dispatch.sol

contract ConstructorFunctionPointerDispatch {
    bytes6 internal name;

    constructor() {
        function(bytes6) internal setter = setName;
        setter("abcdef");

        applyShift(leftByteShift, 3);
    }

    function getName() external view returns (bytes6) {
        return name;
    }

    function setName(bytes6 name_) private {
        name = name_;
    }

    function leftByteShift(bytes6 value, uint256 shift) public pure returns (bytes6) {
        return value << shift * 8;
    }

    function applyShift(
        function(bytes6, uint256) internal returns (bytes6) shiftOperator,
        uint256 bytesToShift
    ) internal {
        name = shiftOperator(name, bytesToShift);
    }
}
