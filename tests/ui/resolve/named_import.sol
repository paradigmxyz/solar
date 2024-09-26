import { MyUdvt } from "./aux/udvt.sol";

struct S {
    MyUdvt value;
}
event Ev(MyUdvt value);
error Er(MyUdvt value);

contract C {
    struct S {
        MyUdvt value;
    }
    event Ev(MyUdvt value);
    error Er(MyUdvt value);
}
