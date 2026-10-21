# Solar: 100× faster Solidity compilation, cheaper contracts

*By DaniPopes, Djordje Todorovic, Georgios Konstantopoulos [byline TBD]*

**Today we are excited to release Solar v0.3 [version TBD], the first version of our Rust Solidity compiler that generates EVM bytecode. Solar compiles Morpho Blue over 100× faster than solc, its contracts are cheaper to deploy and to run, and safe Solidity compiled with Solar beats hand-written assembly.**

Two years ago, [we introduced Solar](https://www.paradigm.xyz/writing/solar) ☀️ as a fast, modular Solidity frontend. Since then, it has grown a full optimizing compiler behind that frontend: a typed middle end that understands Solidity, and a back end that understands the EVM. Specifically:

* **Over 100× faster builds:** Solar compiles Morpho Blue in 0.22 seconds, against 33 seconds for solc.
* **Cheaper contracts by default:** On real-world contracts, Solar's code uses 4% less gas to run, costs 26% less to deploy and is 30% smaller than solc's. On average, it also beats solx and every other Solidity compiler we tested.
* **Safe code that beats assembly:** We rewrote Solady without inline assembly or `unchecked` math. Compiled with Solar, it uses 19% less gas than the original assembly compiled with solc.

Our plan is to make Solar the default compiler in [Foundry](https://github.com/foundry-rs/foundry). If that sounds exciting, read on!

# Solar is fast.

Solar compiles Morpho Blue in 0.22 seconds. solc takes 33.

Compilation is the inner loop of smart contract development. Every `forge build` and `forge test` waits on it, and teams that turn on solc's via-IR pipeline for cheaper code often wait tens of seconds per build. As we [wrote when we announced Solar](https://www.paradigm.xyz/writing/solar), testing was no longer Foundry's bottleneck; the compiler was.

We compiled popular codebases with every Solidity compiler we could find, using the same settings for all of them:

| Project | solc 0.8.37 | solx 0.1.8 | solx 0.2 preview | oksolc 0.2.2 | **Solar** | vs. solc |
|---|---:|---:|---:|---:|---:|---:|
| Morpho Blue | 33.3 s | 4.32 s | 3.62 s | 4.41 s | **0.22 s** | **149×** |
| Solady | 88.2 s | 13.5 s | 9.78 s | 11.2 s | **0.64 s** | **137×** |
| PRBMath | 17.8 s | 4.18 s | fails | 4.53 s | **0.17 s** | **102×** |
| forge-std | 22.9 s | 4.65 s | fails | 4.23 s | **0.23 s** | **100×** |
| OpenZeppelin | 35.5 s | 8.82 s | 4.90 s | 6.10 s | **0.42 s** | **84×** |
| Tempo Zones | 40.4 s | 7.10 s | 11.7 s | 10.3 s | **0.68 s** | **60×** |
| Uniswap v4 core | fails | 8.82 s | 5.98 s | 7.42 s | **0.39 s** | — |
| Seaport | fails | fails | fails | fails | **2.80 s** | — |

*Median of five runs on a dedicated AMD EPYC 9254 server. Every compiler gets the same settings: the via-IR pipeline and the Cancun EVM target, the one configuration they all accept. "Fails" means the compiler gave up, mostly with "stack too deep".*

Solar is 80× faster than solc on average with these settings, and still 30× faster when each project keeps its own settings, where many use solc's quicker legacy pipeline. It also needs a fraction of the memory: 322 MiB to build Solady, where solc needs 2.7 GiB. Solar uses all your cores, but even on a single thread it compiles Morpho Blue 27× faster than solc.

# Solar makes your contracts cheaper.

A fast compiler is only useful if its output is good. So we built a [benchmark suite](https://github.com/paradigmxyz/solar/tree/main/benches/runtime) from real contracts by OpenZeppelin, Solady, Aave, Arbitrum, Maple, Tempo and others. For each one, we deploy every compiler's bytecode to a local node, replay the same transactions, and check that all of them return the same results.

With each project's own settings, Solar beats solc on every one of these workloads. Its contracts cost 26% less to deploy and are 30% smaller, and each transaction uses 4.1% less gas, even counting the 21,000 gas every transaction pays before running any code. The more work a contract does, the bigger the savings: OpenZeppelin's P256 signature verification uses 15% less gas, and Solady's string library 13.5% less.

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

solx needed `unchecked` math to move the storage write out of the loop. Solar does not: it keeps `result` on the stack, with every overflow check intact, and writes storage once at the end, since a revert would undo any earlier write anyway. That makes it 17% cheaper than solc and 11% cheaper than solx. Write the obvious code; Solar makes it cheap.

# How does Solar compare?

We are glad to see more teams investing in Solidity compilers. We measured solc, [solx](https://github.com/NomicFoundation/solx) from the Nomic Foundation (the team behind Hardhat), the October preview of its upcoming Slang-based rewrite, and [oksolc](https://github.com/okcontract/oksolc), a port of solc to Zig, all on the same inputs. Solar compiles whole projects 14–19× faster than each of them, and produces cheaper and smaller code on average. The full results are in our [benchmark suite](https://github.com/paradigmxyz/solar/tree/main/benches/runtime).

# With Solar, safe Solidity beats assembly.

Today, the cheapest Solidity is written in inline assembly with `unchecked` math. Libraries like [Solady](https://github.com/Vectorized/solady) squeeze out every unit of gas by hand, but every line of assembly skips the compiler's checks, and one mistake can cost users their funds. We think the compiler should do this work, not the developer.

To test that idea, we rewrote Solady in plain, checked Solidity ([safe Solady](https://github.com/djolertrk/solady/tree/feat/safe-solady-natspec) [move to paradigmxyz before publishing]). Assembly and `unchecked` remain only in small functions tagged for review, which do what Solidity cannot express, such as reading a raw storage slot or bubbling up a revert. The rewrite keeps every function Solady exposes.

We then ran Solady's own test suite against both versions. They produce the same results, and compiled with Solar, safe Solady uses **19% less gas** than the original assembly compiled with solc, cheaper in nearly every test.

Readable, checked code that is cheaper than hand-written assembly. That is what a compiler should give you.

*[Editorial: this section depends on [paradigmxyz/solar#1999](https://github.com/paradigmxyz/solar/pull/1999) landing before the release.]*

# How is Solar built?

Solar follows the design of modern compilers like rustc and LLVM, where each stage has one job:

```
Solidity → AST → HIR → MIR → EVM IR → bytecode
```

MIR is a typed view of your contract that still knows what a mapping or a checked addition is, so it can remove redundant checks and keep values out of storage and memory. EVM IR sits close to the machine and handles the stack, block layout and peephole rewrites. Keeping each optimization at the level where it belongs makes it small, easy to test and easy to build on.

# Solar will be Foundry's default compiler.

We intend to make Solar the default compiler in Foundry, so that every Foundry user gets these wins without changing a line of code.

Every change to Solar is checked against solc in a differential benchmark, and we run the full test suites of Morpho Blue, Solmate, Solady, Seaport, OpenZeppelin and Uniswap v4 under both compilers, with the same results [re-verify on the release commit].

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

Faster builds, cheaper contracts, and safe code that beats assembly: that is Solar today. [Go try it](https://github.com/paradigmxyz/solar) on your contracts and tell us what you find.

**We're hiring compiler engineers to work on Solar. Reach out with your GitHub and resume to [georgios@paradigm.xyz](mailto:georgios@paradigm.xyz).**

See you on [GitHub](https://github.com/paradigmxyz/solar)!
