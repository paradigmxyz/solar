//@ codegen-matrix: standard view
//@[view] compile-flags: -O gas -Zdump=mir
//@[view] filecheck: --check-prefix=VIEW
//@ run-call: register "name"
//@ run-call: registerTwice "name" => 0x3a81d6fc
//@ run-call: lookupLength "very-long-subdomain-name-with-more-bytes" => 0x0000000000000000000000000000000000000000, 40
//@ run-call: echo "very-long-subdomain-name-with-more-bytes" => "very-long-subdomain-name-with-more-bytes"
//@ run-call: overwrite "name" => "Name"
//@ run-call-fail: 0xf2c298be0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000ffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041
//@ run-call-fail: 0xf2c298be0000000000000000000000000000000000000000000000000000000000000020ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041
//@ run-call-fail: 0xf2c298be00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000064 => 0x
//@ run-call-fail: 0xf2c298be0000000000000000000000000000000000000000000000000000000000000040 => 0x

// Read-only `memory` bytes parameters decode as calldata slices. The decoder
// keeps the allocation it would have copied into, so malformed lengths still
// panic before the data range check and later allocations see the same free
// memory pointer.
contract MemoryBytesView {
    error AlreadyRegistered();

    mapping(string => address) public owners;

    // VIEW-LABEL: fn @register()
    // VIEW: icall @decode_calldata_slice
    // VIEW-NOT: mcopy
    // VIEW: calldatacopy
    // VIEW: keccak256
    function register(string memory name) external {
        if (owners[name] != address(0)) revert AlreadyRegistered();
        owners[name] = msg.sender;
    }

    function registerTwice(string memory name) external returns (bytes4) {
        this.register(name);
        try this.register(name) {} catch (bytes memory reason) {
            return bytes4(reason);
        }
        return 0;
    }

    function lookupLength(string memory name) external view returns (address, uint256) {
        return (owners[name], bytes(name).length);
    }

    function echo(string memory value) external pure returns (string memory) {
        return value;
    }

    // A written parameter keeps its memory copy.
    // VIEW-LABEL: fn @overwrite()
    // VIEW: icall @decode_calldata_type
    // VIEW: mstore8
    // The shared view decoder bumps the free memory pointer without copying.
    // VIEW-LABEL: fn @decode_calldata_slice()
    // VIEW: mload 64
    // VIEW: mstore 64
    // VIEW-NOT: calldatacopy
    // VIEW: ret
    function overwrite(string memory value) external pure returns (string memory) {
        bytes(value)[0] = "N";
        return value;
    }
}
