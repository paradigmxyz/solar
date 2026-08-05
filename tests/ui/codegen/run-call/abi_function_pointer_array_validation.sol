//@ run-call: checkG false => 2
//@ run-call: checkG true => 2
//@ run-call: checkH false => 3
//@ run-call-fail: checkH true
//@ run-call: checkM false => 4
//@ run-call-fail: checkM true
// ported-from: test/libsolidity/semanticTests/abicoder/calldataDecoding/array/calldata_array_function_types_v2.sol
// ported-from: test/libsolidity/semanticTests/abicoder/validation/external_function_type_inside_struct_v2.sol

pragma abicoder v2;

contract AbiFunctionPointerArrayValidation {
    function target() external pure {}

    function g(function() external[] calldata pointers) external pure returns (uint r) {
        r = 2;
    }

    function h(function() external[] calldata pointers) external pure returns (uint r) {
        pointers[0];
        r = 3;
    }

    function m(function() external[] memory pointers) external pure returns (uint r) {
        r = 4;
    }

    function checkG(bool invalid) external view returns (uint) {
        return invoke(this.g.selector, invalid, 2);
    }

    function checkH(bool invalid) external view returns (uint) {
        return invoke(this.h.selector, invalid, 3);
    }

    function checkM(bool invalid) external view returns (uint) {
        return invoke(this.m.selector, invalid, 4);
    }

    function invoke(bytes4 selector, bool invalid, uint expected) internal view returns (uint) {
        function() external[] memory pointers = new function() external[](1);
        pointers[0] = this.target;
        bytes memory data = abi.encodeWithSelector(selector, pointers);
        if (invalid) {
            assembly {
                mstore8(add(add(data, 32), 92), 0x58)
            }
        }
        (bool success, bytes memory result) = address(this).staticcall(data);
        require(success);
        uint value = abi.decode(result, (uint));
        require(value == expected);
        return value;
    }
}
