//@ compile-flags: -Ogas

interface Parser {
    struct Chain {
        string name;
        uint256 chainId;
        string rpcUrl;
    }

    function parseUint(string calldata input) external view returns (uint256);
    function getChain(string calldata input) external view returns (Chain memory);
}

contract StackOnlyCrossBlockRecompute {
    Parser private constant parser = Parser(address(0x1234));

    error InvalidChain(string input);

    function first(string memory input) external view returns (uint256) {
        return resolve(input);
    }

    function second(string memory input) external view returns (uint256) {
        return resolve(input);
    }

    function resolve(string memory input) public view returns (uint256) {
        try parser.parseUint(input) returns (uint256 chainId) {
            return chainId;
        } catch {
            try parser.getChain(input) returns (Parser.Chain memory chain) {
                return chain.chainId;
            } catch {
                revert InvalidChain(input);
            }
        }
    }
}
