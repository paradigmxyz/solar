//@ codegen-matrix: standard
//@ run-call: storageValue => 1
//@ run-call: mappingValue => 1
//@ run-call: freeFunction => 1
//@ run-call: localValue => 105
//@ run-call: storageParameter => 9
// The receiver of an attached call is read at the member access, before the arguments run. This
// pins via-IR: solc 0.8.38 legacy returns 2 for `storageValue`, via-IR returns 1, with and without
// `--optimize` (#2475). A storage array passed to a memory parameter is copied early too, as in
// `call_arguments.sol`; a storage parameter is a reference, so it sees the write.
library L {
    function g(uint256 a, uint256 b) internal pure returns (uint256) {
        return a * 100 + b;
    }

    function f(uint256[] storage m, uint256) internal view returns (uint256) {
        return m[0];
    }
}

function h(uint256 a, uint256 b) pure returns (uint256) {
    return a * 100 + b;
}

using {h} for uint256;

contract Test {
    using L for uint256;
    using L for uint256[];

    uint256 n;
    mapping(uint256 => uint256) m;
    uint256[] sa;

    function bump() internal returns (uint256) {
        n += 1;
        m[1] += 1;
        return n;
    }

    function poke() internal returns (uint256) {
        sa[0] = 9;
        return 0;
    }

    function storageValue() external returns (uint256) {
        return n.g(bump());
    }

    function mappingValue() external returns (uint256) {
        return m[1].g(bump());
    }

    function freeFunction() external returns (uint256) {
        return n.h(bump());
    }

    function localValue() external pure returns (uint256) {
        uint256 x = 1;
        return x.g(x = 5);
    }

    function storageParameter() external returns (uint256) {
        sa.push(1);
        return sa.f(poke());
    }
}
