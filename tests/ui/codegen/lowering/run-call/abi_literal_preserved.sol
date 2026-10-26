//@ codegen-matrix: standard native
//@[mir] compile-flags: --evm-version paris
//@[gas] compile-flags: --evm-version paris
//@[size] compile-flags: --evm-version paris
//@[native] compile-flags: -Ogas --evm-version cancun
//@ run-call: probe => 0xb658c1ef50aec2efb3e26c5587e8ce1ae4d46226f3a28b97bac4fe573717b1f8
//@ run-call: observed => true

contract C {
    function f(bytes memory a, string memory b) external pure returns (bytes32) {
        return keccak256(abi.encode(a, b));
    }

    function probe() external view returns (bytes32) {
        return this.f(hex"0123", "abc");
    }

    function observed() external view returns (bool result) {
        uint256 before;
        assembly {
            before := mload(0x40)
        }
        require(this.f(hex"0123", "abc") == 0xb658c1ef50aec2efb3e26c5587e8ce1ae4d46226f3a28b97bac4fe573717b1f8);
        assembly {
            result := and(eq(mload(before), 2), eq(mload(add(before, 64)), 3))
            result := and(result, eq(mload(add(before, 32)), shl(240, 0x0123)))
            result := and(result, eq(mload(add(before, 96)), shl(232, 0x616263)))
            result := and(result, eq(mload(0x40), add(before, 352)))
        }
    }
}
