struct Person {
    string name;
    uint age;
}

contract B {
    uint c;
    bool x;
}

// Total = 1 + 1 = 2

contract A {
    uint256 a; // 1
    bool b; // 1
    B c; // 1
    Person e; // 1 + 2 fields = 3
    int128 f; // 1
    Person[] g; // 1
    Person[23] h; // 23 * 3 = 69
}

// Total = 1 + 1 + 1 + 3 + 1 + 1 + 69 = 77

contract M {
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

// Total = 4 + 1 + 1 + 1 + 1 = 8
