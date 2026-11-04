//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Inherited::t_get => 511
//@ run-call: FromConstructor::t_get => 511
//@ run-call: TwoArguments::t_get => 11806093279743

// Generated cases. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR:
// base constructor arguments keep raw bits.

contract Base {
    constructor(uint8 x) {
        assembly { sstore(0, x) }
    }
}

contract Base2 {
    constructor(uint8 x, uint16 y) {
        assembly { sstore(0, or(x, shl(16, y))) }
    }
}

function dirty() pure returns (uint8 r) {
    assembly { r := 0x1ff }
}

function dirty16() pure returns (uint16 r) {
    assembly { r := 0xabcd1ff }
}

contract Inherited is Base(dirty()) {
    function t_get() external view returns (uint256 r) {
        assembly { r := sload(0) }
    }
}

contract FromConstructor is Base {
    constructor() Base(dirty()) {}

    function t_get() external view returns (uint256 r) {
        assembly { r := sload(0) }
    }
}

contract TwoArguments is Base2(dirty(), dirty16()) {
    function t_get() external view returns (uint256 r) {
        assembly { r := sload(0) }
    }
}
