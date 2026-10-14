contract C {
    mapping(address payable => uint) m0; //~ ERROR: `address payable` is not allowed as a mapping key type
    mapping(address payable owner => uint balance) m1; //~ ERROR: `address payable` is not allowed as a mapping key type
    mapping(uint => mapping(address payable => uint)) m2; //~ ERROR: `address payable` is not allowed as a mapping key type

    mapping(address => address payable) m3;
    mapping(address => mapping(uint => address payable)) m4;
}
