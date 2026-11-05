uint constant a = 0;
uint constant private b = 0;  //~ ERROR: visibility is not allowed here
uint constant internal c = 0; //~ ERROR: visibility is not allowed here
uint constant public d = 0;   //~ ERROR: visibility is not allowed here
uint constant external e = 0; //~ ERROR: visibility is not allowed here

contract C {
    uint a2 = 0;
    uint private b2 = 0;
    uint internal c2 = 0;
    uint public d2 = 0;
    uint external e2 = 0;     //~ ERROR: `external` not allowed here; allowed values: private, internal, public

    struct S {
        uint a3;
        uint private b3;      //~ ERROR: visibility is not allowed here
        uint internal c3;     //~ ERROR: visibility is not allowed here
        uint public d3;       //~ ERROR: visibility is not allowed here
        uint external e3;     //~ ERROR: visibility is not allowed here
    }

    error Er(
        uint a4,
        uint private b4,      //~ ERROR: visibility is not allowed here
        uint internal c4,     //~ ERROR: visibility is not allowed here
        uint public d4,       //~ ERROR: visibility is not allowed here
        uint external e4      //~ ERROR: visibility is not allowed here
    );

    event Ev(
        uint a5,
        uint private b5,      //~ ERROR: visibility is not allowed here
        uint internal c5,     //~ ERROR: visibility is not allowed here
        uint public d5,       //~ ERROR: visibility is not allowed here
        uint external e5      //~ ERROR: visibility is not allowed here
    );

    function f(
        uint a6,
        uint private b6,      //~ ERROR: visibility is not allowed here
        uint internal c6,     //~ ERROR: visibility is not allowed here
        uint public d6,       //~ ERROR: visibility is not allowed here
        uint external e6      //~ ERROR: visibility is not allowed here
    ) public returns(
        uint a7,
        uint private b7,      //~ ERROR: visibility is not allowed here
        uint internal c7,     //~ ERROR: visibility is not allowed here
        uint public d7,       //~ ERROR: visibility is not allowed here
        uint external e7      //~ ERROR: visibility is not allowed here
    ) {
        uint a8;
        uint private b8;      //~ ERROR: visibility is not allowed here
        uint internal c8;     //~ ERROR: visibility is not allowed here
        uint public d8;       //~ ERROR: visibility is not allowed here
        uint external e8;     //~ ERROR: visibility is not allowed here
        try this.f(0, 0, 0, 0, 0) returns(
            uint a9,
            uint private b9,  //~ ERROR: visibility is not allowed here
            uint internal c9, //~ ERROR: visibility is not allowed here
            uint public d9,   //~ ERROR: visibility is not allowed here
            uint external e9  //~ ERROR: visibility is not allowed here
        ) {} catch {}
    }
}
