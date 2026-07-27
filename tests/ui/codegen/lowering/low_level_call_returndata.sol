//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

interface IERC20Minimal {
    function transfer(address to, uint256 value) external returns (bool);
}

contract LowLevelCallReturndata {
    // CHECK-LABEL: fn @safeTransfer{{[( ]}}
    // CHECK: {{v[0-9]+}} = abi_encode [word, word], selector 0xa9059cbb
    // CHECK: {{v[0-9]+}} = call {{v[0-9]+}}, arg0, 0,
    // CHECK: {{v[0-9]+}} = make_returndata_slice 0,
    // CHECK: returndatacopy
    // CHECK: internal_call @__revert_error
    function safeTransfer(address token, address to, uint256 value) public {
        (bool success, bytes memory data) =
            token.call(abi.encodeWithSelector(IERC20Minimal.transfer.selector, to, value));
        require(success && (data.length == 0 || abi.decode(data, (bool))), "TF");
    }

    // CHECK-LABEL: fn @balanceOf{{[( ]}}
    // CHECK: abi_encode [word], selector 0x70a08231
    // CHECK: {{v[0-9]+}} = staticcall {{v[0-9]+}}, arg0,
    // CHECK: {{v[0-9]+}} = make_returndata_slice 0,
    // CHECK: returndatacopy
    // CHECK: mload
    function balanceOf(address token) public view returns (uint256) {
        (bool success, bytes memory data) =
            token.staticcall(abi.encodeWithSignature("balanceOf(address)", address(this)));
        require(success);
        return abi.decode(data, (uint256));
    }

    // CHECK-LABEL: fn @forward{{[( ]}}
    // CHECK: {{v[0-9]+}} = call {{v[0-9]+}}, arg0, 0,
    // CHECK: {{v[0-9]+}} = make_returndata_slice 0,
    // CHECK: returndatacopy
    // CHECK: internal_call @__ret_bytes
    function forward(address target, bytes memory payload) public returns (bytes memory) {
        (bool success, bytes memory result) = target.call(payload);
        require(success);
        return result;
    }
}
