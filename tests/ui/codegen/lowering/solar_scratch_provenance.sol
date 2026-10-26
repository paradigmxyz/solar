//@ codegen-matrix: standard

contract Test {
    function computed() external pure returns (uint256 result) {
        uint256 pointer;
        assembly { pointer := mload(add(0x20, 0x20)) }
        /// @custom:solar-scratch
        { bytes memory temporary = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(pointer, 0x20)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function narrowed() external pure returns (uint256 result) {
        uint256 raw;
        assembly { raw := mload(0x40) }
        uint256 pointer = uint256(uint160(raw));
        /// @custom:solar-scratch
        { bytes memory temporary = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(pointer, 0x20)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function freePointer() internal pure returns (uint256 result) {
        assembly { result := mload(add(0x20, 0x20)) }
    }

    function forwardedPointer() internal pure returns (uint256) {
        return freePointer();
    }

    function returned() external pure returns (uint256 result) {
        uint256 pointer = forwardedPointer();
        /// @custom:solar-scratch
        { bytes memory temporary = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(pointer, 0x20)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function returnedAndStored() external pure returns (uint256 result) {
        uint256 pointer = freePointer();
        assembly { mstore(0, pointer) } //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        /// @custom:solar-scratch
        { bytes memory temporary = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(mload(0), 0x20)) }
    }

    function pair() internal pure returns (uint160 pointer, uint256 word) {
        pointer = uint160(freePointer());
        word = 9;
    }

    function returnedTuple() external pure returns (uint256 result) {
        (uint160 raw, ) = pair();
        uint256 pointer = uint256(raw);
        /// @custom:solar-scratch
        { bytes memory temporary = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(pointer, 0x20)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }
}
