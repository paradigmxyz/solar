//@compile-flags: -O none -Zdump=mir
//@filecheck:

interface IERC20Minimal {
    function transfer(address to, uint256 value) external returns (bool);
}

contract LowLevelCallReturndata {
    // CHECK-LABEL: fn @safeTransfer{{[( ]}}
    // CHECK: {{v[0-9]+}} = abi_encode [word<u160>, word], object, selector 0xa9059cbb
    // CHECK: [[TARGET:v[0-9]+]] = trunc i256 arg0 to i160
    // CHECK: {{v[0-9]+}} = address_call [[TARGET]],
    // CHECK: {{v[0-9]+}} = icall returndata_bytes<>
    // CHECK: abi_decode [bool]
    function safeTransfer(address token, address to, uint256 value) public {
        (bool success, bytes memory data) =
            token.call(abi.encodeWithSelector(IERC20Minimal.transfer.selector, to, value));
        require(success && (data.length == 0 || abi.decode(data, (bool))), "TF");
    }

    // CHECK-LABEL: fn @balanceOf{{[( ]}}
    // CHECK: abi_encode [word<u160>], object, selector 0x70a08231
    // CHECK: [[TARGET:v[0-9]+]] = trunc i256 arg0 to i160
    // CHECK: {{v[0-9]+}} = address_staticcall [[TARGET]],
    // CHECK: {{v[0-9]+}} = icall returndata_bytes<>
    // CHECK: {{v[0-9]+}} = abi_decode [u256]
    function balanceOf(address token) public view returns (uint256) {
        (bool success, bytes memory data) =
            token.staticcall(abi.encodeWithSignature("balanceOf(address)", address(this)));
        require(success);
        return abi.decode(data, (uint256));
    }

    // CHECK-LABEL: fn @forward{{[( ]}}
    // CHECK: [[TARGET:v[0-9]+]] = trunc i256 arg0 to i160
    // CHECK: {{v[0-9]+}} = address_call [[TARGET]],
    // CHECK: {{v[0-9]+}} = icall returndata_bytes<>
    // CHECK: ret {{v[0-9]+}}
    function forward(address target, bytes memory payload) public returns (bytes memory) {
        (bool success, bytes memory result) = target.call(payload);
        require(success);
        return result;
    }
}
