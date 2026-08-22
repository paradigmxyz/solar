//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: checkG false => 2
//@[gas] run-call: checkG false => 2
//@[size] run-call: checkG false => 2
//@[none] run-call: checkG true => 2
//@[gas] run-call: checkG true => 2
//@[size] run-call: checkG true => 2
//@[none] run-call: checkH false => 3
//@[gas] run-call: checkH false => 3
//@[size] run-call: checkH false => 3
//@[none] run-call-fail: checkH true
//@[gas] run-call-fail: checkH true
//@[size] run-call-fail: checkH true
//@[none] run-call: checkM false => 4
//@[gas] run-call: checkM false => 4
//@[size] run-call: checkM false => 4
//@[none] run-call-fail: checkM true
//@[gas] run-call-fail: checkM true
//@[size] run-call-fail: checkM true
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
