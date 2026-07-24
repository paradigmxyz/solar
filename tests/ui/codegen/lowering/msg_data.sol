//@ check-pass
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=MSGDATA

contract MsgData {
    // `msg.data` is the whole calldata as a lazy slice `(0, calldatasize)`;
    // `.length` reads its length word, indexing reads a calldata byte, and a
    // value use materializes it into memory bytes.
    function len() external pure returns (uint256) {
        return msg.data.length;
    }

    function copy() external pure returns (bytes memory) {
        return msg.data;
    }

    function tail(uint256 a, uint256 b) external pure returns (bytes memory) {
        return msg.data[a:b];
    }
}

// MSGDATA-LABEL: fn @len
// MSGDATA: calldatasize
// MSGDATA-LABEL: fn @copy
// MSGDATA: calldatasize
// MSGDATA: calldatacopy
