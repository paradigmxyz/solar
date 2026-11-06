//@ codegen-matrix: standard
//@ run-call: Shapes::pushAndReadContracts => 2, 0x00000000000000000000000000000000000000aa
//@ run-call: Shapes::pushAndReadSelectors => 1, 2, 0x12345678

// The StdInvariant bookkeeping shapes: storage arrays of addresses and of
// structs with dynamic-array members, written through internal helpers and
// read back through public getters.

contract Shapes {
    struct FuzzSelector {
        address addr;
        bytes4[] selectors;
    }

    address[] private _targetedContracts;
    FuzzSelector[] private _targetedSelectors;

    function targetContract(address a) internal {
        _targetedContracts.push(a);
    }

    function targetSelector(FuzzSelector memory s) internal {
        _targetedSelectors.push(s);
    }

    function targetContracts() public view returns (address[] memory) {
        return _targetedContracts;
    }

    function targetSelectors() public view returns (FuzzSelector[] memory) {
        return _targetedSelectors;
    }

    function pushAndReadContracts() external returns (uint256, address) {
        targetContract(address(0xAA));
        targetContract(address(this));
        address[] memory out = this.targetContracts();
        return (out.length, out[0]);
    }

    function pushAndReadSelectors() external returns (uint256, uint256, bytes4) {
        bytes4[] memory sels = new bytes4[](2);
        sels[0] = 0x12345678;
        sels[1] = 0xabcdef01;
        targetSelector(FuzzSelector({addr: address(this), selectors: sels}));
        FuzzSelector[] memory out = this.targetSelectors();
        return (out.length, out[0].selectors.length, out[0].selectors[0]);
    }
}
