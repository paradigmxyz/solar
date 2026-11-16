//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f => 16
//@ run-call: fArray => true
// ported-from: test/libsolidity/semanticTests/constructor/constructor_function_complex.sol

contract Target {
    uint256 public value;

    constructor(function() external pure returns (uint256) callback) {
        value = callback();
    }
}

contract ArrayTarget {
    uint256 public raw;

    constructor(function() external pure returns (uint256)[1] memory callbacks) {
        assembly {
            sstore(raw.slot, mload(callbacks))
        }
    }
}

contract Caller {
    function f() external returns (uint256) {
        Target target = new Target(this.sixteen);
        return target.value();
    }

    function fArray() external returns (bool) {
        function() external pure returns (uint256)[1] memory callbacks;
        callbacks[0] = this.sixteen;
        ArrayTarget target = new ArrayTarget(callbacks);
        uint256 expected =
            (uint256(uint160(address(this))) << 96) | (uint256(uint32(this.sixteen.selector)) << 64);
        return target.raw() == expected;
    }

    function sixteen() external pure returns (uint256) {
        return 16;
    }
}
