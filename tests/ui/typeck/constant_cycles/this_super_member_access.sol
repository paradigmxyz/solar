//@ check-pass
// The circular constant check follows member accesses on `this` and `super` without crashing.
// TODO: solc rejects these reads with error 7615, which we do not report yet.

contract A {
    function g() public virtual {}
}

contract B is A {
    bytes4 constant Z = this.g.selector;

    function g() public virtual override {}
}

contract C is B {
    bytes4 constant X = this.f.selector;
    bytes4 constant Y = super.g.selector;

    function f() public {}
    function g() public override {}

    function h() public pure returns (bytes4 x, bytes4 y, bytes4 z) {
        assembly {
            x := X
            y := Y
            z := Z
        }
    }
}
