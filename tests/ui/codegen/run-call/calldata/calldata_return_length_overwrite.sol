//@ codegen-matrix: standard
//@ run-call: zeroed 0x1234 => 0x
//@ run-call: shortened 0x123456 => 0x12
//@ run-call: kept 0x1234 => 0x1234

// Returning a memory copy of calldata bytes reuses the calldata slice only while the copy keeps
// the slice's length; a length that assembly rewrites returns the rewritten bytes.
contract CalldataReturnLengthOverwrite {
    function zeroed(bytes calldata data) external pure returns (bytes memory copy) {
        copy = data;
        assembly {
            mstore(copy, 0)
        }
    }

    function shortened(bytes calldata data) external pure returns (bytes memory copy) {
        copy = data;
        assembly {
            mstore(copy, 1)
        }
    }

    function kept(bytes calldata data) external pure returns (bytes memory copy) {
        copy = data;
    }
}
