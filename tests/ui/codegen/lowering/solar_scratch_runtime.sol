//@ codegen-matrix: standard
//@ run-call: digests 0, 0x0000000000000000000000000000000000000000000000000000000000000001 => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: digests 3, 0x0000000000000000000000000000000000000000000000000000000000000001 => 0xab42dc4b571a24a903bc20d8416495db0367ae224e5f9219ac4332a98c2bf3f5
//@ run-call: nested 5 => 234, 0x0000000000000000000000000000000000000000000000000000000000000005
//@ run-call: early 3 => 64
//@ run-call: early 9 => 41
//@ run-call: unchecked_ 7 => 75276140696391174450305814049576319106646922510300487059720162673006384432783
//@ run-call: viewed 0x010203 => 0xfc5bd8809067105864e1fab7f4264472c15e2490664e901d6bbec46c3ce8efda, 5
//@ run-call: helpers 4 => 0x5f9558e31acc2d39ff1a197b3213f2b399243cdf016dcbec66aecbd9c2bf4622
//@ run-call: returnVariable 7 => [7, 32]

// Every function returns what it returns without `@custom:solar-scratch`:
// memory a tagged block allocates is reused after it, while objects allocated
// before the block, like `kept`, and those after it stay intact. A `return`
// out of the block keeps its memory.
import {Bytes} from "solar:core/v1/Bytes.sol";

contract Test {
    using Bytes for bytes;

    function digests(uint256 n, bytes32 salt) public pure returns (bytes32 acc) {
        for (uint256 i; i < n; ++i) {
            /// @custom:solar-scratch
            {
                bytes memory encoded = abi.encode(i, salt, acc);
                acc = keccak256(encoded);
            }
        }
    }

    function nested(uint256 x) public pure returns (uint256 total, bytes memory kept) {
        kept = abi.encode(x);
        /// @custom:solar-scratch
        {
            bytes memory outer = abi.encode(x, x);
            total = outer.length;
            /// @custom:solar-scratch
            {
                bytes memory inner = abi.encode(x, x, x);
                total += inner.length;
            }
            bytes memory after_ = abi.encode(x + 1);
            total += after_.length + uint8(outer[31]);
        }
        bytes memory late = abi.encode(x + 2);
        total += late.length + uint8(kept[31]);
    }

    function early(uint256 x) public pure returns (uint256) {
        /// @custom:solar-scratch
        {
            bytes memory t = abi.encode(x);
            if (x > 5) return t.length + x;
        }
        bytes memory after_ = abi.encode(x, x);
        return after_.length;
    }

    function unchecked_(uint256 x) public pure returns (uint256 y) {
        /// @custom:solar-scratch
        unchecked {
            bytes memory t = abi.encode(x);
            y = uint256(keccak256(t)) + x;
        }
    }

    function viewed(bytes memory data) public pure returns (bytes32 h, uint256 n) {
        /// @custom:solar-scratch
        {
            bytes memory copy = bytes.concat(data, data);
            /// @custom:solar-view
            bytes memory tail = copy.slice(1, copy.length - 1);
            h = keccak256(tail);
            n = tail.length;
        }
    }

    function make(uint256 x) internal pure returns (bytes memory) {
        return abi.encode(x, x);
    }

    function link(bytes[] memory into, uint256 x) internal pure {
        into[0] = make(x);
    }

    function helpers(uint256 x) public pure returns (bytes32 h) {
        /// @custom:solar-scratch
        {
            bytes[] memory local = new bytes[](1);
            link(local, x);
            h = keccak256(local[0]);
        }
        bytes memory after_ = make(x + 1);
        h ^= keccak256(after_);
    }

    // A return variable's default array is made before the block, so it
    // outlives the block's memory.
    function returnVariable(uint256 x) public pure returns (uint256[2] memory r) {
        /// @custom:solar-scratch
        {
            r[0] = x;
            bytes memory t = abi.encode(x);
            r[1] = t.length;
        }
    }
}
