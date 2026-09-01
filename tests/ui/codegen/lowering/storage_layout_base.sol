//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: LayoutSlots::slots => 7, 0, 7, 1, 8, 0
//@ run-call: LayoutStorage::store 99 => 99, 99
//@ run-call: LayoutDerived::slots => 2, 3, 4
//@ run-call: LayoutTransient::slots => 0, 7
//@ run-call: LayoutErc7201::matchesReference => true

function erc7201Reference(string memory namespace) pure returns (uint256) {
    return uint256(
        keccak256(bytes.concat(bytes32(uint256(keccak256(bytes(namespace))) - 1)))
    ) & ~uint256(0xff);
}

contract LayoutSlots layout at 7 {
    int8 private x;
    int32 private y;
    uint256 private z;

    function slots()
        external
        pure
        returns (uint256 xs, uint256 xo, uint256 ys, uint256 yo, uint256 zs, uint256 zo)
    {
        assembly {
            xs := x.slot
            xo := x.offset
            ys := y.slot
            yo := y.offset
            zs := z.slot
            zo := z.offset
        }
    }
}

contract LayoutStorage layout at 42 {
    uint256 private value;

    function store(uint256 newValue) external returns (uint256, uint256 raw) {
        value = newValue;
        assembly {
            raw := sload(42)
        }
        return (value, raw);
    }
}

contract LayoutBase {
    uint256 internal baseValue;
}

contract LayoutMiddle is LayoutBase {
    uint32 internal middleValue;
}

contract LayoutDerived is LayoutMiddle layout at 2 {
    uint256 internal derivedValue;

    function slots() external pure returns (uint256 base, uint256 middle, uint256 derived) {
        assembly {
            base := baseValue.slot
            middle := middleValue.slot
            derived := derivedValue.slot
        }
    }
}

contract LayoutTransient layout at 7 {
    uint256 transient transientValue;
    uint256 persistentValue;

    function slots() external pure returns (uint256 transientSlot, uint256 persistentSlot) {
        assembly {
            transientSlot := transientValue.slot
            persistentSlot := persistentValue.slot
        }
    }
}

contract LayoutErc7201 layout at erc7201("example.main") {
    uint256 private value;

    function matchesReference() external pure returns (bool) {
        uint256 slot;
        assembly {
            slot := value.slot
        }
        return slot == erc7201Reference("example.main");
    }
}
