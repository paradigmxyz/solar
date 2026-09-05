//@ codegen-matrix: standard
//@ run-call: Base::named() => 102
//@ run-call: Base::positional() => 102
//@ run-call: Derived::named() => 201
//@ run-call: Derived::positional() => 201
// A modifier invocation's named arguments bind by the parameter names of the
// modifier the invocation statically refers to, which is the one the type
// checker validated them against. Virtual dispatch then selects the
// implementation to run and gets the arguments positionally, so an override
// that renames same-typed parameters cannot reorder them: the named and the
// positional form of the same call must agree.
//
// `solc` rejects named modifier arguments outright (TYPECK-004), so the
// expected values here are our own positional-equivalence invariant rather
// than something `solc` can confirm directly. They were verified against
// `solc` through the positional form, which it does accept and which must
// produce the same result as the named one.
contract Base {
    uint256 public r;

    modifier m(uint256 a, uint256 b) virtual {
        r = a * 100 + b;
        _;
    }

    function named() public m({a: 1, b: 2}) returns (uint256) {
        return r;
    }

    function positional() public m(1, 2) returns (uint256) {
        return r;
    }
}

contract Derived is Base {
    modifier m(uint256 b, uint256 a) override {
        r = a * 100 + b;
        _;
    }
}
