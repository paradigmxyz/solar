//@ revisions: strict recover
//@[recover] compile-flags: -Zrecover-incomplete-input

struct Token {
    uint256 balance;
}

library Lib {
    struct Record { uint256 amount; }
}

contract C {
    enum Kind { First }
    Token[] tokens;

    function initializer(Token memory token) external pure {
        uint256 b = token.balance
        Token memory next; //~ ERROR: expected one of
        uint8 later = 300; //~[recover] ERROR: mismatched types
    }

    function indexedMember(uint256 i) external view {
        tokens[i].bal //~[recover] ERROR: member `bal` not found
        Token[] memory next; //~[recover] ERROR: expected one of
        uint8 later = 300; //~[recover] ERROR: mismatched types
    }

    function getToken() internal view returns (Token memory) {
        return tokens[0];
    }

    function call() external view {
        getToken().bal //~[recover] ERROR: member `bal` not found
        Lib.Record memory next; //~[recover] ERROR: expected one of
        uint8 later = 300; //~[recover] ERROR: mismatched types
    }

    function chained(Token memory token) external pure {
        uint256 b = token.balance
        Kind first //~[recover] ERROR: expected one of
        Kind second //~[recover] ERROR: expected one of
        Kind third; //~[recover] ERROR: expected one of
        uint8 later = 300; //~[recover] ERROR: mismatched types
    }

    function bareDot(Token memory token) external pure {
        uint256 b = token.
        Kind first //~[recover] ERROR: expected identifier
        //~[recover]^ ERROR: expected one of
        Kind second //~[recover] ERROR: expected one of
        address account; //~[recover] ERROR: expected one of
        uint8 later = 300; //~[recover] ERROR: mismatched types
    }

    function beforeClose(Token memory token) external pure {
        uint256 b = token.balance
        Kind next //~[recover] ERROR: expected one of
    } //~[recover] ERROR: expected one of

    function bareBeforeClose(Token memory token) external pure {
        uint256 b = token.
        Kind next //~[recover] ERROR: expected identifier
        //~[recover]^ ERROR: expected one of
    } //~[recover] ERROR: expected one of

    function functionAddress() external view {
        (this.call).address
        Kind next; //~[recover] ERROR: expected one of
        uint8 later = 300; //~[recover] ERROR: mismatched types
    }

    function addressInitializer() external view {
        address a = this.call.address
        Kind next; //~[recover] ERROR: expected one of
        uint8 later = 300; //~[recover] ERROR: mismatched types
    }

    function comments(Token memory token) external pure {
        uint256 b = token./* member */balance
        Kind /* type */ next /* name */; //~[recover] ERROR: expected one of
        uint8 later = 300; //~[recover] ERROR: mismatched types
    }
}
