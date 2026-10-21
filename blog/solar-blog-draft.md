# Solar: 100× faster Solidity compilation, cheaper contracts

*By DaniPopes, Djordje Todorovic, Georgios Konstantopoulos [byline TBD]*

**Today we are excited to release Solar v0.3 [version TBD], the first version of our Rust Solidity compiler that generates EVM bytecode. Solar compiles real projects over 100× faster than solc, and the contracts it produces are cheaper to deploy and to run.**

Two years ago, [we introduced Solar](https://www.paradigm.xyz/writing/solar) ☀️ as a fast, modular Solidity frontend. Since then it has grown a full optimizing middle and back end, and it now compiles Solidity all the way to bytecode. Specifically:

* **Over 100× faster builds:** Solar compiles Morpho Blue in 0.17 seconds, against 23 seconds for solc: 137× faster. Across nine popular codebases, from OpenZeppelin to Seaport, it is 28× faster on average.
* **Cheaper contracts by default:** On contracts from OpenZeppelin, Solady, Aave, Arbitrum and others, Solar's code uses 4% less gas to run, costs 26% less to deploy, and is 30% smaller than solc's. It also beats solx and every other Solidity compiler we tested.
* **Safe code that beats assembly:** We rewrote Solady without inline assembly or `unchecked` math. Compiled with Solar, the safe version uses 19% less gas than the original assembly compiled with solc.

Our plan is to make Solar the default compiler in [Foundry](https://github.com/foundry-rs/foundry). If that sounds exciting, read on!

# Solar is fast.

Compilation is the inner loop of smart contract development. Every `forge build` and `forge test` waits on it, and as contracts grew more complex, so did the wait. Teams that turn on solc's via-IR pipeline for cheaper code routinely wait tens of seconds per build. As we [wrote when we started Solar](https://www.paradigm.xyz/writing/solar), testing was no longer Foundry's bottleneck; the compiler was.

To measure this, we took each project's full compiler input, exactly as Foundry would send it to solc, kept the project's own settings, and compiled it with every Solidity compiler we could find:

| Project | solc 0.8.37 | solx 0.1.8 | solx 0.2 preview | oksolc 0.2.2 | **Solar** | vs. solc |
|---|---:|---:|---:|---:|---:|---:|
| Morpho Blue (via-IR) | 23.4 s | 3.01 s | 2.51 s | 3.34 s | **0.17 s** | **137×** |
| Tempo Zones (via-IR) | 29.2 s | — | 8.62 s | 7.51 s | **0.48 s** | **61×** |
| Uniswap v4 core (via-IR) | fails¹ | 6.18 s | 4.11 s | 5.53 s | **0.28 s** | — |
| OpenZeppelin 5.6 | 8.64 s | 5.24 s | 3.32 s | — | **0.31 s** | **28×** |
| Solady 0.1.26 | 12.2 s | 8.92 s | 6.72 s | — | **0.54 s** | **23×** |
| Seaport 1.6 | 49.8 s | fails | fails | — | **2.50 s** | **20×** |

*Median of five runs (three for solx) on an AMD EPYC 4585PX with 16 cores. A dash means the compiler does not accept the project: oksolc only supports the via-IR pipeline, and solx only targets Cancun or later, so we set its target to Cancun. ¹solc 0.8.37 stops with "stack too deep" on v4-core.*

The gap is largest where developers feel the most pain: projects on the via-IR pipeline build 60–140× faster. Solar also needs far less memory, peaking at 618 MiB on Seaport where solc needs 2.6 GiB.

Some of this comes from parallelism, since Solar compiles contracts on 8 threads by default and solc uses one. Restricted to a single thread, Solar is still 7× faster than solc on average, and 22× faster on Morpho Blue. The rest comes from a compiler designed for speed from day one, borrowing heavily from rustc.

# Solar makes your contracts cheaper.

A fast compiler is only useful if its output is good. We built a [benchmark suite](https://github.com/paradigmxyz/solar/tree/main/benches/runtime) of real contracts from OpenZeppelin, Solady, Aave, Arbitrum Nitro, Maple, Tempo and others, and replay transaction sequences against each compiler's bytecode on a local node. Every compiler runs the same calls, and every result is checked against the others.

Against solc, Solar wins on all 15 real-world workloads, with 4.1% less gas per transaction, 26% cheaper deployments and 30% smaller bytecode. These figures include the 21,000 gas every transaction pays up front, so the savings in the code itself are larger. The more computation a contract does, the bigger the win:

* Loops over calldata arrays use **48% less gas** than with solc.
* OpenZeppelin's P256 signature verification uses **15% less gas**.
* Solady's LibString uses **13.5% less gas**.
* Tempo's deposit portal is **28% cheaper to deploy** and 6.8 KB smaller.

Developers have learned to hand-optimize around the compiler. Take the first example from [solx's launch post](https://paragraph.com/@zksync/solx-the-new-solidity-compiler-why-it-matters-to-you), a factorial that writes to storage on every iteration:

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

solx needed `unchecked` math to move the storage write out of the loop. Solar does not. It keeps `result` on the stack with every overflow check intact and writes storage once at the end, because a revert would undo any earlier write anyway. Our calls to this contract cost 186,088 gas with Solar, 209,744 with solx and 224,168 with solc. Write the obvious code, and let the compiler do the rest.

# With Solar, safe Solidity beats assembly.

Today, the cheapest Solidity is written in inline assembly with `unchecked` math. Libraries like [Solady](https://github.com/Vectorized/solady) are masterpieces of gas golfing, but every line of assembly skips the compiler's checks, and one mistake can cost users their funds. We think the compiler should do this work, not the developer.

To test that idea, we rewrote all 90 sources of Solady in plain, checked Solidity ([safe Solady](https://github.com/djolertrk/solady/tree/feat/safe-solady-natspec) [move to paradigmxyz before publishing]). Assembly and `unchecked` remain only in small functions tagged for review, about one in ten, which do what Solidity cannot express, such as reading a raw storage slot or bubbling up a revert. The rewrite keeps every function Solady exposes and passes Solady's own test suite.

Then we compared it against the original. Compiled with Solar, safe Solady uses **19% less gas** than the original assembly compiled with solc, measured on Solady's own tests with identical inputs, and it is cheaper in nearly every one of them. On a separate benchmark of individual library calls, each checked against an independent model of the function, it uses **41% less gas** than the best solc build of the original.

Readable, checked code that is cheaper than hand-written assembly: this is the future we want for smart contracts.

*[Editorial: this section depends on [paradigmxyz/solar#1999](https://github.com/paradigmxyz/solar/pull/1999) landing before the release.]*

# How does Solar compare?

We are glad to see more teams investing in Solidity compilers, and we measured them all on the same inputs:

* **solc** is the reference compiler. Solar matches its language and output formats, and beats it on compile time, gas and size on every project in our suite.
* **solx**, the LLVM-based compiler developed by the Nomic Foundation (the team behind Hardhat), focuses on gas. Against solx 0.1.8, Solar compiles 19× faster on whole projects, uses 1.4% less gas and emits 40% smaller code. solx is ahead on one of our 13 workloads, P256, by 2.7%.
* **solx 0.2**, Nomic's upcoming rewrite on Slang and MLIR, is faster than 0.1.8 but still 13× slower than Solar. Its October preview wins three workloads, each by under 1%, with code almost twice the size of Solar's, and cannot yet build Seaport, forge-std or PRBMath.
* **oksolc** is a port of solc to Zig. It produces the same bytecode as solc, so Solar's gas and size wins carry over, and Solar compiles whole projects 18× faster.

# How is Solar built?

Solar follows the design of modern compilers like rustc and LLVM. Each stage has one job:

```
Solidity → AST → HIR → MIR → EVM IR → bytecode
```

* **MIR** is a typed, SSA-based view of your contract. It still knows what a mapping, an array or a checked addition is, so it can remove redundant checks, keep storage values on the stack across loops, inline functions and fold constants, all with full knowledge of Solidity's semantics.
* **EVM IR** sits close to the machine: blocks of opcodes with explicit stack operations. It schedules the stack, lays out blocks, merges duplicate code and applies peephole rewrites, all priced by a single model of gas and code size.

Because every optimization lives at the level where it belongs, each one is small, easy to test, and easy to build on. Solar's frontend already powers `forge lint` and Foundry's language server today. Its new middle and back end are what make everything below possible.

# Solar will be Foundry's default compiler.

We intend to make Solar the default compiler in Foundry, so that every `forge build` and `forge test` gets these wins without changing a line of code.

Correctness comes first. Every change to Solar runs against solc in a differential benchmark, and we run the full test suites of Morpho Blue, Solmate, Solady, Seaport, OpenZeppelin and Uniswap v4 under both compilers, with the same results [re-verify on the release commit].

## OK, I'm sold! How do I try Solar?

You can use Solar in any Foundry project today:

```bash
# [install command TBD]
SOLC_WRAPPER=1 FOUNDRY_SOLC=$(which solar) forge test
```

If your project does not build, or a test behaves differently than with solc, please [open an issue](https://github.com/paradigmxyz/solar/issues). This is the most valuable feedback you can give us right now.

# Where do we go from here?

Since announcing Solar, we have:

1. [Introduced Solar](https://www.paradigm.xyz/writing/solar) as a fast Solidity frontend.
2. Integrated Solar into Foundry, powering `forge lint` and `forge lsp` [confirm].
3. Built a typed middle end (MIR) and a machine-level back end (EVM IR).
4. Released full code generation, faster and cheaper than solc (this post).

Next, we are most excited about:

* **Security from the compiler:** Our IRs know far more about a contract than its bytecode does. We will use them to prove that optimizations preserve behavior, extend the Lean proofs we already have for our rewrite rules, and flag dangerous patterns before they ship. Safe Solady is the first step: Solar can already report which contracts contain no assembly or `unchecked` code outside reviewed functions.
* **A Solar standard library**, so that safe primitives need no assembly at all.
* **Debug info** through [ethdebug](https://github.com/ethdebug/format), so optimized contracts stay easy to debug.
* **LLM-assisted optimizations**, where models propose transformations and the compiler proves them correct before applying them.

In the meantime, [go try Solar](https://github.com/paradigmxyz/solar) on your contracts and tell us what you find.

**We're hiring compiler engineers to work on Solar. Reach out with your GitHub and resume to [georgios@paradigm.xyz](mailto:georgios@paradigm.xyz).**

See you on [GitHub](https://github.com/paradigmxyz/solar)!
