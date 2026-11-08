//@ codegen-matrix: standard
//@ run-call: C::dynField => 1
//@ run-call: C::structField => 1
//@ run-call: C::fixedField => 1

// A memory struct literal whose field argument lives in storage must
// materialize a memory copy. Struct member types keep their declared storage
// flavor, and lowering once handed back the raw storage slot as the field
// value, so every later read treated the slot number as a memory pointer.

contract C {
    bytes4[] internal sels;
    uint256[3] internal words;

    struct Inner {
        uint64 a;
        uint64 b;
    }

    Inner internal inner;

    struct WithDyn {
        address addr;
        bytes4[] sel;
    }

    struct WithInner {
        uint256 tag;
        Inner inner;
    }

    struct WithFixed {
        uint256 tag;
        uint256[3] vals;
    }

    function dynField() external returns (uint256) {
        sels.push(this.dynField.selector);
        sels.push(this.structField.selector);
        WithDyn memory s = WithDyn({addr: address(this), sel: sels});
        require(s.sel.length == 2, "len");
        require(s.sel[0] == this.dynField.selector, "e0");
        require(s.sel[1] == this.structField.selector, "e1");
        return 1;
    }

    function structField() external returns (uint256) {
        inner.a = 7;
        inner.b = 9;
        WithInner memory s = WithInner({tag: 1, inner: inner});
        require(s.inner.a == 7 && s.inner.b == 9, "inner");
        inner.a = 100;
        require(s.inner.a == 7, "copy");
        return 1;
    }

    function fixedField() external returns (uint256) {
        words[0] = 11;
        words[1] = 22;
        words[2] = 33;
        WithFixed memory s = WithFixed({tag: 2, vals: words});
        require(s.vals[0] == 11 && s.vals[1] == 22 && s.vals[2] == 33, "vals");
        return 1;
    }
}
