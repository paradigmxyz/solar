//@ codegen-matrix: standard
//@ run-call: sum ([4, 5, 6], [7]) => 25
//@ run-call: sum ([], [7]) => 8
//@ run-call: first ([4, 5, 6], [7]) => 4
//@ run-call: word (3, ([4, 5], [7])) => 3
//@ run-call: nested (3, ([4, 5], [7])) => 12

// A struct field named `offset` is an ordinary member outside inline
// assembly. Only the Yul `.offset` suffix reads a calldata pointer.
struct Params {
    uint256[] offset;
    uint256[] length;
}

struct Word {
    uint256 offset;
    Params length;
}

contract CalldataStructOffsetField {
    function sum(Params calldata p) external pure returns (uint256 t) {
        t = total(p.offset) + p.length[0] + p.length.length;
        if (p.offset.length != 0) t += p.offset[2] - p.offset[0];
    }

    function first(Params calldata p) external pure returns (uint256 v) {
        uint256[] calldata a = p.offset;
        assembly {
            v := calldataload(a.offset)
        }
    }

    function word(Word calldata w) external pure returns (uint256) {
        return w.offset;
    }

    function nested(Word calldata w) external pure returns (uint256) {
        return w.length.offset[1] + w.length.length[0];
    }

    function total(uint256[] calldata a) public pure returns (uint256 t) {
        for (uint256 i; i < a.length; i++) t += a[i];
    }
}
