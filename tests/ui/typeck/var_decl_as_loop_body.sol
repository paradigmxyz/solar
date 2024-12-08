contract C {
    function var_decl_inside_loops() external {
        for (uint256 i = 0; i < 100; ++i) {
            uint256 m_count = i + 1 * 2;
        }

        for (uint256 i = 0; i < 100; ++i) uint256 m_count = i + 1 * 2; //~ ERROR: variable declaration statements are not allowed as the body of a loop (for, while, do while), meaning they must be inside of a block

        for (uint256 i = 0; i < 100; ++i)
            for (uint256 j = 0; i < 100; ++j) {
                uint256 k = i + j;
            }

        for (uint256 i = 0; i < 100; ++i)
            for (uint256 j = 0; i < 100; ++j) uint256 k = i + j; //~ ERROR: variable declaration statements are not allowed as the body of a loop (for, while, do while), meaning they must be inside of a block

        while (true) uint256 x = 4; //~ ERROR: variable declaration statements are not allowed as the body of a loop (for, while, do while), meaning they must be inside of a block

        do uint256 x = 4; while (true); //~ ERROR: variable declaration statements are not allowed as the body of a loop (for, while, do while), meaning they must be inside of a block

        unchecked {
            {
                {
                    for (uint256 i = 0; i < 10; ++i) uint256 y = 0; //~ ERROR: variable declaration statements are not allowed as the body of a loop (for, while, do while), meaning they must be inside of a block
                }
            }
        }
    }
}
