//@ codegen-matrix: standard
//@ run-call: forged 0x1122334455 => 0, 5
//@ run-call: below 0x1122334455 => 0, 0x1122334455000000000000000000000000000000000000000000000000000000

// Assembly that forges a pointer or reads below a new object may find the
// memory copy of a `bytes` argument, so the argument keeps its copy.
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
}
