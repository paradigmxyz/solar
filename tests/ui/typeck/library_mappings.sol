contract L {
    function f(mapping(uint=>uint) storage x, mapping(uint=>uint) storage y) internal {
        x = y;
    }
}
