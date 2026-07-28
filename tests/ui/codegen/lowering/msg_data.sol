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

    // These builtins used to hit `lower_builtin`'s catch-all zero.
    // CHECK-LABEL: fn @environment{{[( ]}}
    // CHECK: coinbase
    // CHECK: timestamp
    // CHECK: prevrandao
    // CHECK: number
    // CHECK: gaslimit
    // CHECK: chainid
    // CHECK: basefee
    // CHECK: blobbasefee
    // CHECK: calldataload 0
    // CHECK: origin
    // CHECK: gasprice
    // CHECK: gas
    function environment() external view returns (uint256 value) {
        value ^= uint160(address(block.coinbase));
        value ^= block.timestamp;
        value ^= block.prevrandao;
        value ^= block.number;
        value ^= block.gaslimit;
        value ^= block.chainid;
        value ^= block.basefee;
        value ^= block.blobbasefee;
        value ^= uint32(msg.sig);
        value ^= uint160(tx.origin);
        value ^= tx.gasprice;
        value ^= gasleft();
    }

    // CHECK-LABEL: fn @tail{{[( ]}}
    // CHECK: calldatasize
    // CHECK: make_calldata_slice
    // CHECK: calldatacopy
    function tail(uint256 a, uint256 b) external pure returns (bytes memory) {
        return msg.data[a:b];
    }
}
