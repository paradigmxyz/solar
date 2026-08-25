//@ revisions: mir runtime
//@[mir] compile-flags: -O none -Zdump=mir
//@[mir] filecheck:
//@[runtime] run-call: compareBytes32 0x0000000000000000000000000000000000000000000000000000000000000002, 0x0000000000000000000000000000000000000000000000000000000000000001 => true
//@[runtime] run-call: compareBytes32 0x0000000000000000000000000000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000000000000000000000000000002 => false
//@[runtime] run-call: compareAddress 2, 1 => true
//@[runtime] run-call: compareAddress 1, 2 => false
//@[runtime] run-call: compareAddress 0x0000000000000000000000010000000000000000000000000000000000000000, 0 => false
//@[runtime] run-call: compareInt256 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0 => true
//@[runtime] run-call: compareInt256 1, 0 => false

contract InternalFunctionPointerAssemblyCast {
    // CHECK-LABEL: fn @compareBytes32{{[( ]}}
    // CHECK: internal_call @__internal_dispatch_0
    function compareBytes32(bytes32 a, bytes32 b) external pure returns (bool) {
        function(bytes32, bytes32) pure returns (bool) source = _bytes32Greater;
        function(uint256, uint256) pure returns (bool) target;
        assembly {
            target := source
        }
        return target(uint256(a), uint256(b));
    }

    function _bytes32Greater(bytes32 a, bytes32 b) private pure returns (bool) {
        return a > b;
    }

    // CHECK-LABEL: fn @compareAddress{{[( ]}}
    // CHECK: internal_call @__internal_dispatch_0
    // CHECK-LABEL: fn @__internal_dispatch_0{{[( ]}}
    // CHECK: internal_call @_bytes32Greater
    // CHECK: internal_call @_addressGreater
    function compareAddress(uint256 a, uint256 b) external pure returns (bool) {
        function(address, address) pure returns (bool) source = _addressGreater;
        function(uint256, uint256) pure returns (bool) target;
        assembly {
            target := source
        }
        return target(a, b);
    }

    function _addressGreater(address a, address b) private pure returns (bool) {
        return a > b;
    }

    function compareInt256(uint256 a, uint256 b) external pure returns (bool) {
        function(int256, int256) pure returns (bool) source = _int256Less;
        function(uint256, uint256) pure returns (bool) target;
        assembly {
            target := source
        }
        return target(a, b);
    }

    function _int256Less(int256 a, int256 b) private pure returns (bool) {
        return a < b;
    }
}
