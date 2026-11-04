struct T {
    uint8[] a;
}

type U is uint;

function eq(U a, U b) pure returns (bool) {
    return U.unwrap(a) == U.unwrap(b);
}

function neg(U a) pure returns (U) {
    return U.wrap(~U.unwrap(a));
}

using {eq as ==, neg as ~} for U global;

contract C {
    uint x;
    uint[] arr;
    address owner;
    mapping(uint => uint) m;
    enum E { A, B }
    struct S { uint a; uint b; }
    uint constant K = 1;

    function g() internal returns (uint) {
        x++;
        return 1;
    }

    event Ev();
    error Er();

    function noEffect(uint a, uint b, bool c) public view {
        require; //~ WARN: statement has no effect
        assert; //~ WARN: statement has no effect
        msg.sender; //~ WARN: statement has no effect
        block.timestamp; //~ WARN: statement has no effect
        1; //~ WARN: statement has no effect
        1 + 2; //~ WARN: statement has no effect
        true; //~ WARN: statement has no effect
        "str"; //~ WARN: statement has no effect
        K; //~ WARN: statement has no effect
        (a == b, "msg"); //~ WARN: statement has no effect
        (msg.sender == owner, "not owner"); //~ WARN: statement has no effect
        a == b; //~ WARN: statement has no effect
        a < b && c; //~ WARN: statement has no effect
        !c; //~ WARN: statement has no effect
        ~a; //~ WARN: statement has no effect
        a << 1; //~ WARN: statement has no effect
        c ? a : b; //~ WARN: statement has no effect
        g; //~ WARN: statement has no effect
        this.noEffect; //~ WARN: statement has no effect
        C; //~ WARN: statement has no effect
        uint; //~ WARN: statement has no effect
        E.A; //~ WARN: statement has no effect
        S; //~ WARN: statement has no effect
        Ev; //~ WARN: statement has no effect
        Er; //~ WARN: statement has no effect
        uint8(a); //~ WARN: statement has no effect
        address(this); //~ WARN: statement has no effect
        payable(msg.sender); //~ WARN: statement has no effect
        S(1, 2); //~ WARN: statement has no effect
        type(uint).max; //~ WARN: statement has no effect
        [a, b]; //~ WARN: statement has no effect
        (((1))); //~ WARN: statement has no effect
    }

    function placeholders(uint a, uint b) public view {
        // Mentioning a variable silences unused variable warnings, and solc accepts it.
        a;
        (a, b);
        x;
        (a);
        this;
        super;
    }

    function calldataCopies(uint8[] calldata cd, uint8[] memory mem, bool c) public pure {
        c ? cd : mem;
        T(cd);
        [cd, mem];
    }

    function noWarn(uint a, uint b, uint[] memory mem, uint[] calldata cd, E e, S calldata s, int i) public {
        g();
        x = a;
        x += 1;
        x++;
        --x;
        delete x;
        arr.push(1);
        a + 1;
        a - b;
        a * b;
        a ** b;
        -i;
        a / b;
        a % b;
        mem[a];
        cd[a];
        arr[a];
        m[a];
        cd[a:];
        s.a;
        cd.length;
        E(a);
        (a == b, g());
        (a, x = 1);
        a == b ? g() : 0;
        payable(owner).send(0);
        keccak256("");
        abi.encode(a);
        new C();
        U u;
        u == u;
        ~u;
        emit Ev();
    }
}
