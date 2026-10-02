//@ codegen-matrix: standard
//@ run-call: forged 0x1122334455 => 0, 5
//@ run-call: below 0x1122334455 => 0, 0x1122334455000000000000000000000000000000000000000000000000000000
//@[gas] run-call: moved 0x61616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161 => 0x0000000000000000000000000000000000000000000000000000000000000046, 0
//@[size] run-call: moved 0x61616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161 => 0x0000000000000000000000000000000000000000000000000000000000000046, 0

// Assembly that forges a pointer, reads below a new object, or moves the free
// memory pointer may find the memory copy of a `bytes` argument, so the
// argument keeps its copy.
contract MemoryBytesViewAssembly {
    mapping(bytes => uint256) public m;

    function forged(bytes memory b) external view returns (uint256 l, uint256 n) {
        l = m[b];
        bytes memory r;
        assembly {
            r := sub(mload(0x40), 0x40)
        }
        n = r.length;
    }

    function below(bytes memory b) external view returns (uint256 l, bytes32 v) {
        l = m[b];
        bytes memory t = new bytes(1);
        assembly {
            v := mload(add(t, not(31)))
        }
    }

    function moved(bytes memory b) external view returns (bytes32 r, uint256 l) {
        l = m[b];
        assembly {
            mstore8(0x5e, 0)
            mstore8(0x5f, 0x80)
        }
        bytes memory x = new bytes(0);
        assembly {
            r := mload(add(x, 0x40))
        }
    }
}
