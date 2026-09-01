//@ codegen-matrix: standard
//@ run-call: C::keepsLocal 1, 2, 3, 4, 5 => 42
//@ run-call: C::forwards => 1

// A reassigned Solidity local must survive assembly that scribbles
// compiler-owned low memory — the pattern hand-written forwarding proxies use
// (`mstore(0x40, returndatasize())`, `calldatacopy(0, 0, calldatasize())`,
// `delegatecall(gas(), ...)`). solc keeps such a local on the stack; we spill
// reassigned locals to a fixed frame slot, so frame-slot promotion must lift
// the local back to SSA or the calldata copy fuses garbage into it.

contract Impl {
    function echo(uint256 x) external pure returns (uint256) {
        return x;
    }
}

contract C {
    function keepsLocal(uint256 a, uint256 b, uint256 c, uint256 d, uint256 e)
        external
        pure
        returns (uint256 r)
    {
        uint256 x = 1;
        if (a > 0) {
            x = 42;
        }
        assembly {
            calldatacopy(0, 0, calldatasize())
        }
        // `x` must be 42, not calldata bytes fused into a clobbered slot.
        r = x + b - b + c - c + d - d + e - e;
    }

    function forwards() external returns (uint256) {
        Impl impl = new Impl();
        address target = address(impl);
        bytes memory cd = abi.encodeWithSignature("echo(uint256)", uint256(7));
        uint256 ok;
        assembly {
            let p := mload(0x40)
            calldatacopy(0, 0, calldatasize())
            let success :=
                delegatecall(gas(), target, add(cd, 0x20), mload(cd), 0x00, 0x20)
            ok := and(success, eq(mload(0x00), 7))
        }
        require(ok == 1, "forward");
        return 1;
    }
}
