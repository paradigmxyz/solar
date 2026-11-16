uint a = 0;                   //~ ERROR: only constant variables are allowed at file level
uint constant b = 0;
uint immutable c = 0;         //~ ERROR: only constant variables are allowed at file level

contract C {
    uint a2 = 0;
    uint constant b2 = 0;
    uint immutable c2 = 0;

    struct S {
        uint a3;
        uint constant b3;     //~ ERROR: mutability is not allowed here
        uint immutable c3;    //~ ERROR: mutability is not allowed here
    }

    error Er(
        uint a4,
        uint constant b4,     //~ ERROR: mutability is not allowed here
        uint immutable c4     //~ ERROR: mutability is not allowed here
    );

    event Ev(
        uint a5,
        uint constant b5,     //~ ERROR: mutability is not allowed here
        uint immutable c5     //~ ERROR: mutability is not allowed here
    );

    function f(
        uint a6,
        uint constant b6,     //~ ERROR: mutability is not allowed here
        uint immutable c6     //~ ERROR: mutability is not allowed here
    ) public returns(
        uint a7,
        uint constant b7,     //~ ERROR: mutability is not allowed here
        uint immutable c7     //~ ERROR: mutability is not allowed here
    ) {
        uint a8;
        uint constant b8;     //~ ERROR: mutability is not allowed here
        uint immutable c8;    //~ ERROR: mutability is not allowed here
        try this.f(0, 0, 0) returns(
            uint a9,
            uint constant b9, //~ ERROR: mutability is not allowed here
            uint immutable c9 //~ ERROR: mutability is not allowed here
        ) {} catch {}
    }
}
