//@compile-flags: -O none -Zdump=mir
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
    // CHECK-NEXT: [[SRC:v[0-9]+]] = make_calldata_slice 0,
    // CHECK: [[BYTES:v[0-9]+]] = alloc
    // CHECK: [[DST:v[0-9]+]] = ptradd [[BYTES]], 32
    // CHECK-NEXT: [[PTR:v[0-9]+]] = extract_value calldataslice, [[SRC]], 0
    // CHECK-NEXT: [[LEN:v[0-9]+]] = extract_value calldataslice, [[SRC]], 1
    // CHECK-NEXT: copy calldata [[DST]], [[PTR]], [[LEN]]
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
    // CHECK: load calldata
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
    // CHECK: [[SRC:v[0-9]+]] = make_calldata_slice
    // CHECK: [[BYTES:v[0-9]+]] = alloc
    // CHECK: [[DST:v[0-9]+]] = ptradd [[BYTES]], 32
    // CHECK-NEXT: [[PTR:v[0-9]+]] = extract_value calldataslice, [[SRC]], 0
    // CHECK-NEXT: [[LEN:v[0-9]+]] = extract_value calldataslice, [[SRC]], 1
    // CHECK-NEXT: copy calldata [[DST]], [[PTR]], [[LEN]]
    function tail(uint256 a, uint256 b) external pure returns (bytes memory) {
        return msg.data[a:b];
    }
}
