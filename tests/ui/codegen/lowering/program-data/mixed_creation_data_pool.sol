//@ codegen-matrix: standard
//@ run-call: CopyBeforeCreate::fromBytes 0 => 1, true, true
//@ run-call: CopyBeforeCreate::direct 0 => 1, true
//@ run-call: CopyBeforeCreate::fromBytes 41 => 42, true, true
//@ run-call: CopyBeforeCreate::direct 41 => 42, true
//@ run-call-fail: CopyBeforeCreate::fromBytes 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: CopyBeforeCreate::direct 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: CreateBeforeCopy::fromBytes 0 => 1, true, true
//@ run-call: CreateBeforeCopy::direct 0 => 1, true
//@ run-call: CreateBeforeCopy::fromBytes 41 => 42, true, true
//@ run-call: CreateBeforeCopy::direct 41 => 42, true
//@ run-call-fail: CreateBeforeCopy::fromBytes 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: CreateBeforeCopy::direct 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

contract MixedDataChild {
    uint256 public value;

    constructor(uint256 input) {
        value = input + 1;
    }
}

abstract contract ObserveOwnCode {
    // Observe the emitted program so a later physical data-pack pass cannot supply deduplication.
    function codeMatches() internal view returns (bool matches) {
        assembly {
            let size := codesize()
            let buffer := mload(64)
            codecopy(buffer, 0, size)
            matches := eq(keccak256(buffer, size), extcodehash(address()))
        }
    }
}

// Exercise both declaration orders without assuming a particular child bytecode length.
contract CopyBeforeCreate is ObserveOwnCode {
    function fromBytes(uint256 input) external returns (uint256 value, bool padding, bool observed) {
        bytes memory code = type(MixedDataChild).creationCode;
        assembly {
            padding := 1
            let length := mload(code)
            let remainder := and(length, 31)
            if remainder {
                let tail := mload(add(add(code, 32), and(length, not(31))))
                let mask := sub(shl(mul(sub(32, remainder), 8), 1), 1)
                padding := iszero(and(tail, mask))
            }
        }
        bytes memory init = abi.encodePacked(code, abi.encode(input));
        address child;
        assembly {
            child := create(0, add(init, 32), mload(init))
            if iszero(child) {
                returndatacopy(0, 0, returndatasize())
                revert(0, returndatasize())
            }
        }
        value = MixedDataChild(child).value();
        observed = codeMatches();
    }

    function direct(uint256 input) external returns (uint256 value, bool observed) {
        MixedDataChild child = new MixedDataChild(input);
        value = child.value();
        observed = codeMatches();
    }

}

contract CreateBeforeCopy is ObserveOwnCode {
    function direct(uint256 input) external returns (uint256 value, bool observed) {
        MixedDataChild child = new MixedDataChild(input);
        value = child.value();
        observed = codeMatches();
    }

    function fromBytes(uint256 input) external returns (uint256 value, bool padding, bool observed) {
        bytes memory code = type(MixedDataChild).creationCode;
        assembly {
            padding := 1
            let length := mload(code)
            let remainder := and(length, 31)
            if remainder {
                let tail := mload(add(add(code, 32), and(length, not(31))))
                let mask := sub(shl(mul(sub(32, remainder), 8), 1), 1)
                padding := iszero(and(tail, mask))
            }
        }
        bytes memory init = abi.encodePacked(code, abi.encode(input));
        address child;
        assembly {
            child := create(0, add(init, 32), mload(init))
            if iszero(child) {
                returndatacopy(0, 0, returndatasize())
                revert(0, returndatasize())
            }
        }
        value = MixedDataChild(child).value();
        observed = codeMatches();
    }

}
