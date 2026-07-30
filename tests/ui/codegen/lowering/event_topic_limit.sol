//@compile-flags: -Zcodegen -Zdump=mir

contract EventTopicLimit {
    event Named( //~ ERROR: event cannot have more than 3 indexed parameters
        uint256 indexed a,
        uint256 indexed b,
        uint256 indexed c,
        uint256 indexed d
    );

    event Anonymous( //~ ERROR: event cannot have more than 4 indexed parameters
        uint256 indexed a,
        uint256 indexed b,
        uint256 indexed c,
        uint256 indexed d,
        uint256 indexed e
    ) anonymous;

    function emitNamed() external {
        emit Named(1, 2, 3, 4);
        emit Named(1, 2, 3, 4);
    }

    function emitAnonymous() external {
        emit Anonymous(1, 2, 3, 4, 5);
        emit Anonymous(1, 2, 3, 4, 5);
    }
}
