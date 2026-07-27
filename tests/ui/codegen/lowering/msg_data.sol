//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract MsgData {
    // `msg.data` is the whole calldata as a lazy slice `(0, calldatasize)`;
    // `.length` reads its length word, indexing reads a calldata byte, and a
    // value use materializes it into memory bytes.
    // CHECK-LABEL: fn @len{{[( ]}}
    // CHECK: calldatasize
    function len() external pure returns (uint256) {
        return msg.data.length;
    }

    // CHECK-LABEL: fn @copy{{[( ]}}
    // CHECK: calldatasize
    // CHECK: calldatacopy
    function copy() external pure returns (bytes memory) {
        return msg.data;
    }

    // CHECK-LABEL: fn @tail{{[( ]}}
    // CHECK: calldatasize
    // CHECK: make_calldata_slice
    // CHECK: calldatacopy
    function tail(uint256 a, uint256 b) external pure returns (bytes memory) {
        return msg.data[a:b];
    }
}
