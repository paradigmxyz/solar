# Solar: 100× faster Solidity compilation, cheaper contracts

*By DaniPopes, Djordje Todorovic, Georgios Konstantopoulos [byline TBD]*

**Today we are excited to release Solar v0.3 [version TBD], the first version of our Rust Solidity compiler that generates EVM bytecode. Solar compiles Morpho Blue over 100× faster than solc, and the contracts it produces are cheaper to deploy and to run.**

Two years ago, [we introduced Solar](https://www.paradigm.xyz/writing/solar) ☀️ as a fast, modular Solidity frontend. Since then, it has grown a full optimizing compiler behind that frontend: a typed middle end that understands Solidity, and a back end that understands the EVM. Specifically:

* **Over 100× faster builds:** Solar compiles Morpho Blue in [0.17] seconds, against [23] seconds for solc.
* **Cheaper contracts by default:** On real-world contracts, Solar's code uses [4%] less gas to run, costs [26%] less to deploy and is [30%] smaller than solc's. On average, it also beats solx and every other Solidity compiler we tested.
* **Safe code that beats assembly:** We rewrote Solady without inline assembly or `unchecked` math. Compiled with Solar, it uses [19%] less gas than the original assembly compiled with solc.

Our plan is to make Solar the default compiler in [Foundry](https://github.com/foundry-rs/foundry). If that sounds exciting, read on!

# Solar is fast.

Compilation is the inner loop of smart contract development. Every `forge build` and `forge test` waits on it, and teams that turn on solc's via-IR pipeline for cheaper code often wait tens of seconds per build. As we [wrote two years ago](https://www.paradigm.xyz/writing/solar), testing was no longer Foundry's bottleneck; the compiler was.

We compiled popular codebases exactly as Foundry would, with every Solidity compiler we could find:

[TABLE: whole-project compile times, shared config, from the clean box run]

Across all [N] projects, Solar is [X]× faster than solc on average, and it needs a fraction of the memory: [618 MiB] on Seaport, where solc needs [2.6 GiB]. Solar uses all your cores, but even on a single thread it compiles Morpho Blue [22]× faster than solc.

# Solar makes your contracts cheaper.

A fast compiler is only useful if its output is good. So we built a [benchmark suite](https://github.com/paradigmxyz/solar/tree/main/benches/runtime) from real contracts by OpenZeppelin, Solady, Aave, Arbitrum, Maple, Tempo and others. For each one, we deploy every compiler's bytecode to a local node, replay the same transactions, and check that all of them return the same results.

Solar wins on every one of these workloads. Its contracts cost [26%] less to deploy and are [30%] smaller, and each transaction uses [4.1%] less gas, even counting the 21,000 gas every transaction pays before running any code. The more work a contract does, the bigger the savings: OpenZeppelin's P256 signature verification uses [15%] less gas, and Solady's string library [13.5%] less.

Developers have learned to hand-optimize around the compiler, and Solar makes much of that unnecessary. Take the first example from [solx's launch post](https://paragraph.com/@zksync/solx-the-new-solidity-compiler-why-it-matters-to-you), a factorial that writes to storage on every iteration:

```solidity
contract FactorialStorage {
    uint256 public result;

    function computeFactorial(uint256 n) external {
        result = 1;
        for (uint256 i = 2; i <= n; ++i) {
            result *= i;
        }
    }
}
```

solx needed `unchecked` math to move the storage write out of the loop. Solar does not: it keeps `result` on the stack, with every overflow check intact, and writes storage once at the end, since a revert would undo any earlier write anyway. That makes it [17%] cheaper than solc and [11%] cheaper than solx. Write the obvious code, and let the compiler do the rest.

# With Solar, safe Solidity beats assembly.

Today, the cheapest Solidity is written in inline assembly with `unchecked` math. Libraries like [Solady](https://github.com/Vectorized/solady) are masterpieces of gas golfing, but every line of assembly skips the compiler's checks, and one mistake can cost users their funds. We think the compiler should do this work, not the developer.

To test that idea, we rewrote Solady in plain, checked Solidity ([safe Solady](https://github.com/djolertrk/solady/tree/feat/safe-solady-natspec) [move to paradigmxyz before publishing]). Assembly and `unchecked` remain only in small functions tagged for review, which do what Solidity cannot express, such as reading a raw storage slot or bubbling up a revert. The rewrite keeps every function Solady exposes and passes the same Solady tests as the original.

Then we ran Solady's own test suite against both. Compiled with Solar, safe Solady uses **[19%] less gas** than the original assembly compiled with solc, and it is cheaper in nearly every test.

Readable, checked code that is cheaper than hand-written assembly: this is the future we want for smart contracts.

*[Editorial: this section depends on [paradigmxyz/solar#1999](https://github.com/paradigmxyz/solar/pull/1999) landing before the release.]*

# How does Solar compare?

We are glad to see more teams investing in Solidity compilers. We measured solc, [solx](https://github.com/NomicFoundation/solx) from the Nomic Foundation (the team behind Hardhat), its upcoming Slang-based rewrite, and [oksolc](https://github.com/okcontract/oksolc), a port of solc to Zig, all on the same inputs. Solar compiles [13–19]× faster than any of them on whole projects, and produces cheaper and smaller code on average. The full results are in our [benchmark suite](https://github.com/paradigmxyz/solar/tree/main/benches/runtime).

# How is Solar built?

Solar follows the design of modern compilers like rustc and LLVM, where each stage has one job:

```
Solidity → AST → HIR → MIR → EVM IR → bytecode
```

MIR is a typed view of your contract that still knows what a mapping or a checked addition is, so it can remove redundant checks and keep values out of storage and memory. EVM IR sits close to the machine and handles the stack, block layout and peephole rewrites. Keeping each optimization at the level where it belongs makes it small, easy to test and easy to build on, and it is what makes our roadmap possible.

# Solar will be Foundry's default compiler.

We intend to make Solar the default compiler in Foundry, so that every `forge build` and `forge test` gets these wins without changing a line of code. Solar's frontend already powers `forge lint` and Foundry's language server today.

Correctness comes first. Every change to Solar is checked against solc in a differential benchmark, and we run the full test suites of Morpho Blue, Solmate, Solady, Seaport, OpenZeppelin and Uniswap v4 under both compilers, with the same results [re-verify on the release commit].

## OK, I'm sold! How do I try Solar?

You can use Solar in any Foundry project today:

```bash
# [install command TBD]
SOLC_WRAPPER=1 FOUNDRY_SOLC=$(which solar) forge test
```

If your project does not build, or a test behaves differently than with solc, please [open an issue](https://github.com/paradigmxyz/solar/issues). This is the most valuable feedback you can give us right now.

# Where do we go from here?

In the last two years, we have:

1. [Announced Solar](https://www.paradigm.xyz/writing/solar) as a fast Solidity frontend.
2. Integrated Solar into Foundry, powering `forge lint` and `forge lsp` [confirm].
3. Built a typed middle end (MIR) and a machine-level back end (EVM IR).
4. Released full code generation, faster and cheaper than solc (this post).

Next, we are most excited about:

* **Security from the compiler:** Our IRs know far more about a contract than its bytecode does. We will use them to prove that optimizations preserve behavior, extend the Lean proofs we already have for our rewrite rules, and flag dangerous patterns before they ship. Safe Solady is the first step, and Solar will report which contracts contain no assembly or `unchecked` code outside reviewed functions.
* **A Solar standard library**, so that safe primitives need no assembly at all.
* **Debug info** through [ethdebug](https://github.com/ethdebug/format), so optimized contracts stay easy to debug.
* **LLM-assisted optimizations**, where models propose transformations and the compiler proves them correct before applying them.

In the meantime, [go try Solar](https://github.com/paradigmxyz/solar) on your contracts and tell us what you find.

**We're hiring compiler engineers to work on Solar. Reach out with your GitHub and resume to [georgios@paradigm.xyz](mailto:georgios@paradigm.xyz).**

See you on [GitHub](https://github.com/paradigmxyz/solar)!
