contract C {
    mapping(address => fixed) m; //~ ERROR: fixed-point types are not supported
}
