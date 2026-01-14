// https://github.com/paradigmxyz/solar/issues/197

abstract contract A {
    modifier mod() virtual;

    function f() public mod {}
}

contract B is A {
    modifier mod() virtual override { _; }
}
