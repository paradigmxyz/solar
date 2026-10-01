//@ compile-flags: -Ogas --allow=2018 --emit=bin
//@ normalize-stdout-test: "(?s).+" -> ""

// A resident caller prefix plus the callee's stack arguments must fall back to frame-backed
// arguments when emitting the tuple would require DUP17.

library Constants {
    uint256 internal constant WAD = 1e18;
    uint256 internal constant BPS = 10_000;
}

interface Types {
    struct Entry {
        address account;
        uint8 decimals;
    }
    struct Path {
        address[] accounts;
    }
    struct State {
        mapping(address => Entry) entries;
    }
    struct Result {
        uint256 value;
    }
}

library PathBuilder {
    function build(
        Types.State storage,
        address,
        address,
        address,
        uint8,
        uint8,
        address,
        address
    ) internal view returns (Types.Path memory path) {}
}

library MathA {
    int256 internal constant Q = 1e9;

    function bits(uint256 word) internal pure returns (uint256) {
        return (word >> 232) & 0xffff;
    }

    function delta(
        address,
        uint256 word,
        uint256 x,
        uint256 y,
        uint256 i,
        uint256 j,
        uint256 k,
        uint256 a,
        uint256 b
    ) internal pure returns (int256) {
        int256 first = combine(a, b, k, j, x);
        return combine(a, b, k, j, y) - first;
    }

    function combine(uint256 a, uint256 b, uint256 k, uint256 j, uint256 x)
        private
        pure
        returns (int256)
    {
        return int256(a ^ b ^ k ^ j ^ x);
    }
}

library Repro {
    function lookup(uint8, uint16) internal view returns (uint256 value) {}
    function direction(uint128, uint128) internal pure returns (int8) {}

    function values(uint256, uint32, address, uint256, uint256)
        private
        view
        returns (uint256 value, uint256 i, uint256 j, uint256 k, uint256 a, uint256 b)
    {}

    function rate(uint32, uint16, uint32) internal pure returns (uint32 result) {}
    function applyValue(uint256, int256) internal pure returns (uint256 result) {}

    function scaled(int256 value, uint256 word, uint32 divisor) private pure returns (int256) {
        return (value * int256(uint256(divisor))) / (int256(MathA.bits(word)) * MathA.Q);
    }

    function pair(uint256 amount, uint32 divisor, address account, uint256 word, int8 sign)
        internal
        view
        returns (uint256 first, uint256 second)
    {
        (first,,,,,) = values(amount, divisor, account, word, exponent(word, sign));
    }

    function manyArgs(
        uint256 amount,
        uint32 divisor,
        uint256 word,
        uint256 x,
        uint256 numerator,
        uint256 denominator,
        bool capped,
        uint256 fallbackValue,
        address account,
        uint256 i,
        uint256 j,
        uint256 k,
        uint256 a,
        uint256 b
    ) internal view returns (uint256 result) {
        uint256 ratio = (numerator * Constants.BPS) / denominator;
        uint256 target;
        int256 difference = int256(target) - int256(x);
        result = difference == 0
            ? fallbackValue
            : applyValue(
                amount,
                scaled(
                    MathA.delta(account, word, x, target, i, j, k, a, b) / difference,
                    word,
                    divisor
                )
            );
        if (capped && result > amount) result = amount;
    }

    function exponent(uint256, int8) internal pure returns (uint256) {}

    struct Config {
        uint128 low;
        uint128 high;
        uint16 scale;
        uint32 offset;
        uint8 decimals;
        uint8 kind;
        uint16 window;
        address account;
        uint256 cached;
    }

    struct Accumulator {
        uint256 value;
        uint32 count;
    }

    struct Context {
        uint256 numerator;
        address sentinel;
        uint8 decimals;
        address account;
        uint256 word;
    }

    function run(Types.State storage state, address first, address second, uint256 numerator)
        external
        view
        returns (Types.Result memory result)
    {
        return prepare(state, first, second, numerator, true);
    }

    function prepare(
        Types.State storage state,
        address first,
        address second,
        uint256 numerator,
        bool enabled
    ) internal view returns (Types.Result memory) {
        Context memory context;
        Config memory left = config(state.entries[first], first != context.sentinel);
        Config memory right = config(state.entries[second], second != context.sentinel);
        return walk(state, first, second, numerator, enabled, context, left, right);
    }

    function walk(
        Types.State storage state,
        address first,
        address second,
        uint256 numerator,
        bool enabled,
        Context memory context,
        Config memory left,
        Config memory right
    ) private view returns (Types.Result memory result) {
        Types.Path memory path = PathBuilder.build(
            state,
            first,
            second,
            context.sentinel,
            left.decimals,
            right.decimals,
            left.account,
            right.account
        );
        iterate(state, path, context, result, left, right, enabled);
    }

    function config(Types.Entry storage, bool) private view returns (Config memory result) {}

    function iterate(
        Types.State storage state,
        Types.Path memory path,
        Context memory context,
        Types.Result memory result,
        Config memory left,
        Config memory right,
        bool enabled
    ) private view {
        uint256 length = path.accounts.length;
        for (uint256 i = 0; i < length - 1; i++) {
            Accumulator memory accumulator = step(state, path, i, context, left, right, enabled);
        }
    }

    function step(
        Types.State storage state,
        Types.Path memory path,
        uint256 index,
        Context memory context,
        Config memory left,
        Config memory right,
        bool enabled
    ) private view returns (Accumulator memory accumulator) {
        uint256 numerator = context.numerator;
        uint256 last = path.accounts.length - 1;
        address current = path.accounts[index];
        address next = path.accounts[index + 1];
        address sentinel = context.sentinel;
        bool forward = current != sentinel
            && (index == 0 ? left.account : state.entries[current].account) == next;
        address selected = forward ? current : next;
        bool indirect = forward ? index != 0 : index + 1 != last;
        Config memory selectedConfig = indirect
            ? config(state.entries[selected], true)
            : forward ? left : right;
        uint256 adjacent = forward ? index + 1 : index;
        uint8 adjacentDecimals = adjacent == 0
            ? right.decimals
            : path.accounts[adjacent] == sentinel
                ? context.decimals
                : state.entries[path.accounts[adjacent]].decimals;
        uint256 amount = initial(
            indirect ? lookup(selectedConfig.kind, selectedConfig.window) : selectedConfig.cached,
            accumulator,
            context
        );
        uint256 pairValue;
        if (indirect) {
            (accumulator.value, pairValue) = firstCall(
                selectedConfig,
                context.account,
                context.word,
                numerator,
                amount,
                forward,
                accumulator
            );
            (accumulator.value, pairValue) = failingCaller(
                selectedConfig,
                context.account,
                context.word,
                numerator,
                amount,
                accumulator.count,
                forward,
                adjacentDecimals
            );
        }
    }

    function firstCall(
        Config memory selected,
        address account,
        uint256 word,
        uint256,
        uint256 amount,
        bool,
        Accumulator memory accumulator
    ) private view returns (uint256 value, uint256 pairValue) {
        uint256 other;
        (pairValue, other) = pair(
            amount,
            rate(accumulator.count, selected.scale, selected.offset),
            account,
            word,
            direction(selected.low, selected.high)
        );
    }

    function initial(uint256, Accumulator memory, Context memory)
        private
        view
        returns (uint256 amount)
    {}

    function failingCaller(
        Config memory selected,
        address account,
        uint256 word,
        uint256 numerator,
        uint256 amount,
        uint32 count,
        bool capped,
        uint8 adjacentDecimals
    ) private view returns (uint256 value, uint256 fallbackValue) {
        uint256 denominator = selected.low == 0 ? 1 : uint256(selected.low);
        uint32 divisor = rate(count, selected.scale, selected.offset);
        uint256 x = exponent(word, direction(selected.low, selected.high));
        (uint256 pairValue, uint256 i, uint256 j, uint256 k, uint256 a, uint256 b) =
            values(amount, divisor, account, word, x);
        fallbackValue = pairValue;
        if (capped) {
            uint256 result = manyArgs(
                amount,
                divisor,
                word,
                x,
                numerator,
                denominator,
                true,
                pairValue,
                account,
                i,
                j,
                k,
                a,
                b
            );
            return ((numerator * result) / Constants.WAD, fallbackValue);
        }
        uint256 quotient = (numerator * Constants.WAD) / fallbackValue;
        int256 decimalDelta = int256(uint256(selected.decimals))
            - int256(uint256(adjacentDecimals));
        uint256 adjusted = decimalDelta == 0
            ? quotient
            : decimalDelta > 0
                ? quotient * (10 ** uint256(decimalDelta))
                : quotient / (10 ** uint256(-decimalDelta));
        uint256 result = manyArgs(
            amount,
            divisor,
            word,
            x,
            adjusted,
            denominator,
            false,
            pairValue,
            account,
            i,
            j,
            k,
            a,
            b
        );
    }
}
