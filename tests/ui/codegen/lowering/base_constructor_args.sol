//@ run-call: BaseConstructorArgs::value(); constructor=[5] => 11
//@ run-call: BaseConstructorArgs::labelHash(); constructor=[5] => 0x14502d3ab34ae28d404da8f6ec0501c6f295f66caa41e122cfa9b1291bc0f9e8
//@ run-call: ConstructorArgumentOrder::order() => 1, 3, 2, 4
//@ run-call: ConstructorInitializationOrder::x() => 4
//@ run-call: NamedDerived::value() => 12
//@ run-call: BaseConstructorReturn::value() => 2
// ported-from: test/libsolidity/semanticTests/constructor/order_of_evaluation.sol
// ported-from: test/libsolidity/semanticTests/inheritance/constructor_inheritance_init_order_3_legacy.sol

contract Root {
    uint256 public value;
    string internal label;

    constructor(uint256 value_, string memory label_) {
        value = value_;
        label = label_;
    }
}

contract Middle is Root {
    constructor(uint256 value_, string memory label_) Root(value_ + 1, label_) {}
}

contract BaseConstructorArgs is Middle {
    constructor(uint256 value_) Middle(value_ * 2, "ok") {}

    function labelHash() external view returns (bytes32) {
        return keccak256(bytes(label));
    }
}

contract OrderA {
    constructor(uint256) {}
}

contract OrderB {
    constructor(uint256) {}
}

contract OrderC {
    constructor(uint256) {}
}

contract OrderD {
    constructor(uint256) {}
}

contract ConstructorArgumentOrder is OrderD, OrderC, OrderB, OrderA {
    uint256[] internal values;

    constructor() OrderA(record(1)) OrderC(record(2)) OrderB(record(3)) OrderD(record(4)) {}

    function record(uint256 value_) internal returns (uint256) {
        values.push(value_);
        return value_;
    }

    function order() external view returns (uint256, uint256, uint256, uint256) {
        return (values[0], values[1], values[2], values[3]);
    }
}

contract InitializationBase {
    uint256 public x = 2;

    constructor(uint256) {}

    function touch() internal returns (uint256) {
        x = 4;
        return x;
    }
}

contract ConstructorInitializationOrder is InitializationBase {
    constructor() InitializationBase(touch()) {}
}

contract NamedBase {
    uint256 public value;

    constructor(uint256 a, uint256 b) {
        value = a * 10 + b;
    }
}

contract NamedDerived is NamedBase({b: 2, a: 1}) {}

contract ReturningBase {
    uint256 public value;

    constructor() {
        value = 1;
        return;
        value = 3;
    }
}

contract BaseConstructorReturn is ReturningBase {
    constructor() {
        value = 2;
    }
}
