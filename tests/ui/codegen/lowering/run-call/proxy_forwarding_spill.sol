//@ codegen-matrix: standard
//@ run-call: FrameForwarding::derived 7,4096 => 22
//@ run-call: FrameForwarding::sweep 7,4096 => 7
//@ run-call: FrameForwarding::sweep 7,0 => 7
//@ run-call: Harness::run => 1
//@ run-call: FrameForwarding::run 7, 4096 => 7
//@ run-call: FrameForwarding::branch 7, 4096, true => 8
//@ run-call: FrameForwarding::branch 7, 4096, false => 9

// A forwarding proxy sets the free-memory pointer low and
// `calldatacopy(0, 0, calldatasize())` before `delegatecall`ing the copied
// calldata. The compiler spilled the implementation address to a low slot the
// copy overwrites, then reloaded it after the copy — delegatecalling a garbage
// (zero) address. With calldata under one word the slots are missed and the
// proxy works, so the bug only shows for a >128-byte forwarded call. The
// backend must keep the value stack-resident across the clobber and never
// stage the call's own operands into its forwarded input range.

contract Impl {
    uint256 public v;

    function setBig(uint256 a, uint256 b, uint256 c, bytes calldata d) external {
        v = a + b + c + d.length;
    }
}

contract Proxy {
    bytes32 internal immutable _defaultImplementation;
    bytes32 internal constant _SLOT =
        0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc;

    constructor(address impl) {
        _defaultImplementation = bytes32(uint256(uint160(impl)));
    }

    fallback() external payable {
        bytes32 implementation;
        assembly {
            mstore(0x40, returndatasize())
            implementation := sload(_SLOT)
        }
        if (implementation == bytes32(0)) {
            implementation = _defaultImplementation;
        }
        assembly {
            calldatacopy(returndatasize(), returndatasize(), calldatasize())
            if iszero(
                delegatecall(
                    gas(), implementation, returndatasize(), calldatasize(), codesize(), returndatasize()
                )
            ) {
                returndatacopy(0x00, 0x00, returndatasize())
                revert(0x00, returndatasize())
            }
            returndatacopy(0x00, 0x00, returndatasize())
            return(0x00, returndatasize())
        }
    }
}

contract Harness {
    function run() external returns (uint256) {
        Impl impl = new Impl();
        Proxy p = new Proxy(address(impl));
        // 4 + 3*32 (a,b,c) + 32 (offset) + 32 (len) = 196 bytes > 128, so the
        // forwarded copy overwrites the compiler's low spill slots.
        Impl(address(p)).setBig(10, 20, 30, hex"deadbeefdeadbeefdeadbeefdeadbeef");
        require(Impl(address(p)).v() == 76, "forward");
        return 1;
    }
}

contract FrameForwarding {
    function run(uint256 value, uint256 length) external pure returns (uint256) {
        return forward(value, length);
    }

    function forward(uint256 value, uint256 length) internal pure returns (uint256) {
        assembly { calldatacopy(0x80, calldatasize(), length) }
        return value;
    }

    function branch(uint256 value, uint256 length, bool first) external pure returns (uint256) {
        return forwardBranch(value, length, first);
    }

    function forwardBranch(uint256 value, uint256 length, bool first) internal pure returns (uint256) {
        assembly { calldatacopy(0x80, calldatasize(), length) }
        if (first) return value + 1;
        return value + 2;
    }

    function derived(uint256 value, uint256 length) external pure returns (uint256) {
        return forwardDerived(value, length);
    }

    function forwardDerived(uint256 value, uint256 length) internal pure returns (uint256) {
        uint256 derivedValue = value * 3 + 1;
        assembly { calldatacopy(0x80, calldatasize(), length) }
        return derivedValue;
    }

    function sweep(uint256 value, uint256 length) external pure returns (uint256) {
        return forwardSweep(value, length);
    }

    function forwardSweep(uint256 value, uint256 length) internal pure returns (uint256) {
        assembly {
            for { let ptr := 0 } lt(ptr, length) { ptr := add(ptr, 32) } {
                mstore(ptr, 0)
            }
        }
        return value;
    }
}
