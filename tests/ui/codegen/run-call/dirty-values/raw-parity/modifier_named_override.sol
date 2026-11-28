//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call-fail: Derived::write() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// A named modifier argument binds by the parameter names of the declaration the invocation
// refers to, and reaches the overriding modifier's parameter, whatever its name. The dirty
// `uint8` keeps its raw bits through that parameter, so the storage array index panics. solc
// rejects named modifier arguments; the expectation is its via-IR result for the positional
// `m(dirty())`.

function dirty() pure returns (uint8 v) {
    assembly { v := 257 }
}

contract Base {
    modifier m(uint8 a) virtual {
        _;
    }

    function write() external m({a: dirty()}) {}
}

contract Derived is Base {
    uint256[2] slots;

    modifier m(uint8 b) override {
        slots[b] = 42;
        _;
    }
}
