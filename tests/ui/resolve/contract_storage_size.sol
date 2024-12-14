contract B {
    uint b;
}

contract A {
    struct Person {
        string first;
        string middle;
        string last;
    }

    Person my;
    mapping(string => uint256) public a;
    Person[] public b;
    bool c;
    B d;
}
