uint constant b = 0;

contract Test {
    bool b;

    function f(int b) private {
        uint b;
        {
            string memory b;
        }
        for (bytes32 b; false;) {
            bytes31 b;
        }
    }

    function g(uint x) private
        returns (int x) //~ ERROR: already declared
    {}
}
