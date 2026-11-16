struct S {
    uint x;
}

enum E {
    A
}

contract C {
    function f0(
        uint memory a2, //~ ERROR: data location can only be specified for array, struct or mapping types
        uint[] memory b2,
        S memory c2,
        S[] memory d2,
        E memory e2, //~ ERROR: data location can only be specified for array, struct or mapping types
        E[] memory f22
    ) private {}
    function f1(
        uint memory a2, //~ ERROR: data location can only be specified for array, struct or mapping types
        uint[] memory b2,
        S memory c2,
        S[] memory d2,
        E memory e2, //~ ERROR: data location can only be specified for array, struct or mapping types
        E[] memory f22
    ) internal {}
    function f2(
        uint memory a2, //~ ERROR: data location can only be specified for array, struct or mapping types
        uint[] memory b2,
        S memory c2,
        S[] memory d2,
        E memory e2, //~ ERROR: data location can only be specified for array, struct or mapping types
        E[] memory f22
    ) public {}
    function f3(
        uint memory a2, //~ ERROR: data location can only be specified for array, struct or mapping types
        uint[] memory b2,
        S memory c2,
        S[] memory d2,
        E memory e2, //~ ERROR: data location can only be specified for array, struct or mapping types
        E[] memory f22
    ) external {}

    function r0() private returns (
        uint memory a2, //~ ERROR: data location can only be specified for array, struct or mapping types
        uint[] memory b2,
        S memory c2,
        S[] memory d2,
        E memory e2, //~ ERROR: data location can only be specified for array, struct or mapping types
        E[] memory f22
    ) {}
    function r1() internal returns (
        uint memory a2, //~ ERROR: data location can only be specified for array, struct or mapping types
        uint[] memory b2,
        S memory c2,
        S[] memory d2,
        E memory e2, //~ ERROR: data location can only be specified for array, struct or mapping types
        E[] memory f22
    ) {}
    function r2() public returns (
        uint memory a2, //~ ERROR: data location can only be specified for array, struct or mapping types
        uint[] memory b2,
        S memory c2,
        S[] memory d2,
        E memory e2, //~ ERROR: data location can only be specified for array, struct or mapping types
        E[] memory f22
    ) {}
    function r3() external returns (
        uint memory a2, //~ ERROR: data location can only be specified for array, struct or mapping types
        uint[] memory b2,
        S memory c2,
        S[] memory d2,
        E memory e2, //~ ERROR: data location can only be specified for array, struct or mapping types
        E[] memory f22
    ) {}
}
