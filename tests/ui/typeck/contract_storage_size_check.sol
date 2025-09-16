//@ compile-flags: -Zprint-max-storage-sizes

struct Person {
    string name;
    uint age;
}

contract B {
    uint c;
    bool x;
}

contract A { //~ NOTE: :A requires a maximum of 77 storage slots
    uint256 a; // 1
    bool b; // 1
    B c; // 1
    Person e; // 1 + 2 fields = 3
    int128 f; // 1
    Person[] g; // 1
    Person[23] h; // 23 * 3 = 69
}

contract M { //~ NOTE: :M requires a maximum of 8 storage slots
    struct P1 {
        string first;
        string middle;
        string last;
    }

    P1 my; // 4
    mapping(string => uint256) public a; // 1
    P1[] public b; // 1
    bool c; // 1
    B d; // 1
}

contract Random { //~ NOTE: :Random requires a maximum of 38 storage slots
    struct Rec {
        Rec[] r;
    }

    Rec r; // 1 + 1

    function() internal internal $0; // 1

    string a; // 1
    bytes b; // 1
    string[] c; // 1
    uint[10][] d; // 1
    bool e; // 1
    uint[][10] f; // 1 * 10

    struct One {
        uint x;
    }

    One[10] g; // 2 * 10
}
