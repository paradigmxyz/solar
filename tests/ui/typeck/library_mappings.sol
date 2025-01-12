contract L {
    function f(mapping(uint=>uint) storage x, mapping(uint=>uint) storage y) internal {
        // TODO: disallow assignment
        x = y;
    }
}
