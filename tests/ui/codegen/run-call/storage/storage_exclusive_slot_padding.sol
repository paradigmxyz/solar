//@ codegen-matrix: standard
//@ run-call: stateVariable() => 0
//@ run-call: arrayElement() => 0
//@ run-call: arrayPush() => 0
//@ run-call: mappingValue() => 0
//@ run-call: structField() => 0
//@ run-call: singleElement() => 0
//@ run-call: sharedSlot() => 7

// A value that owns its slot is written as the whole word, so dirty padding that assembly left
// above it is cleared. solc updates the field in place and keeps the padding. A value that shares
// its slot still keeps its neighbors. See CODEGEN-025 in docs/SOLC_DIVERGENCE.md.
contract C {
    address owner;
    address[] addrs;
    mapping(uint256 => address) owners;
    struct S {
        address a;
        uint256 b;
    }
    S s;
    uint8[1] single;
    address first;
    uint8 second;

    function stateVariable() external returns (uint256 r) {
        assembly {
            sstore(owner.slot, not(0))
        }
        owner = address(1);
        assembly {
            r := shr(160, sload(owner.slot))
        }
    }

    function arrayElement() external returns (uint256 r) {
        addrs.push(address(1));
        assembly {
            mstore(0, addrs.slot)
            sstore(keccak256(0, 32), not(0))
        }
        addrs[0] = address(2);
        assembly {
            mstore(0, addrs.slot)
            r := shr(160, sload(keccak256(0, 32)))
        }
    }

    function arrayPush() external returns (uint256 r) {
        addrs.push(address(1));
        addrs.pop();
        assembly {
            mstore(0, addrs.slot)
            sstore(keccak256(0, 32), not(0))
        }
        addrs.push(address(2));
        assembly {
            mstore(0, addrs.slot)
            r := shr(160, sload(keccak256(0, 32)))
        }
    }

    function mappingValue() external returns (uint256 r) {
        owners[3] = address(1);
        bytes32 slot;
        assembly {
            mstore(0, 3)
            mstore(32, owners.slot)
            slot := keccak256(0, 64)
            sstore(slot, not(0))
        }
        owners[3] = address(2);
        assembly {
            r := shr(160, sload(slot))
        }
    }

    function structField() external returns (uint256 r) {
        assembly {
            sstore(s.slot, not(0))
        }
        s.a = address(1);
        assembly {
            r := shr(160, sload(s.slot))
        }
    }

    function singleElement() external returns (uint256 r) {
        assembly {
            sstore(single.slot, not(0))
        }
        single[0] = 1;
        assembly {
            r := shr(8, sload(single.slot))
        }
    }

    function sharedSlot() external returns (uint256) {
        second = 7;
        first = address(1);
        return second;
    }
}
