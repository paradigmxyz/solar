# Solar compiles Solidity 28× faster, into cheaper contracts

*By DaniPopes, Djordje Todorovic, Georgios Konstantopoulos [byline TBD]*

**Today we are releasing Solar v0.3 [version TBD], the first release of our Rust Solidity compiler that turns your contracts into EVM bytecode. Two years ago we [introduced Solar](https://www.paradigm.xyz/writing/solar) as a fast frontend. Today it is a full optimizing compiler. Three things to remember:**

1. **Builds are 28× faster.** Solar compiles nine popular projects 28× faster than solc on average. Morpho Blue goes from 23 seconds to 0.17 seconds: **137× faster**.
2. **Contracts are cheaper.** On real-world contracts, Solar's code costs **4% less gas** to run, **26% less** to deploy, and is **30% smaller** than solc's. On average it also beats solx and every other Solidity compiler we measured.
3. **Safe code beats assembly.** We rewrote Solady, the go-to library for gas-optimized Solidity, without inline assembly or `unchecked` math. Compiled with Solar, it uses **19% less gas** than the original assembly compiled with solc, on Solady's own test suite.

Our goal is to make Solar the default compiler in [Foundry](https://github.com/foundry-rs/foundry). Read on for the numbers, how we got them, and what comes next.

## Solar compiles your project before you can blink.

Compile time is the inner loop of smart contract development. Every `forge test` waits on it, and projects built with solc's via-IR pipeline routinely wait tens of seconds per build.

We took each project's full compiler input, exactly as Foundry hands it to solc, with the project's own settings, and compiled it with every Solidity compiler we could find:

| Project | solc 0.8.37 | solx 0.1.8 | solx 0.2 preview | oksolc 0.2.2 | **Solar** | vs solc |
|---|---:|---:|---:|---:|---:|---:|
| Morpho Blue (via-IR) | 23.4 s | 3.01 s | 2.51 s | 3.34 s | **0.17 s** | **137×** |
| Tempo Zones (via-IR) | 29.2 s | — | 8.62 s | 7.51 s | **0.48 s** | **61×** |
| Uniswap v4 core (via-IR) | fails¹ | 6.18 s | 4.11 s | 5.53 s | **0.28 s** | — |
| OpenZeppelin 5.6 | 8.64 s | 5.24 s | 3.32 s | — | **0.31 s** | **28×** |
| Solady 0.1.26 | 12.2 s | 8.92 s | 6.72 s | — | **0.54 s** | **23×** |
| Seaport 1.6 | 49.8 s | fails | fails | — | **2.50 s** | **20×** |

Median of five runs (three for solx) on an AMD EPYC 4585PX with 16 cores. A dash means the compiler does not accept the project: oksolc only supports the via-IR pipeline, and solx only targets Cancun or later, so we set its target to Cancun. ¹ solc 0.8.37 stops with "stack too deep" on v4-core's via-IR build.

Across all nine projects solc can build, Solar is 28× faster. It also uses far less memory: Seaport peaks at 618 MiB with Solar against 2.6 GiB with solc.

Part of this comes from parallelism. Solar compiles contracts on 8 threads by default, while solc uses one. But even on a single thread, Solar is 7× faster than solc on average, and 22× faster on Morpho Blue.

## Solar's contracts are cheaper to run and to deploy.

A fast compiler is only useful if its output is good. We built a benchmark suite of contracts from OpenZeppelin, Solady, Aave, Arbitrum Nitro, Maple, Tempo Zones and others, and replay real transaction sequences against each compiler's bytecode on a local node:

| vs. | Runtime gas | Deployment gas | Bytecode size | Workloads where Solar wins on gas |
|---|---:|---:|---:|---:|
| solc 0.8.37 | **−4.1%** | **−26%** | **−30%** | 15 of 15 |
| solx 0.1.8 | **−1.4%** | **−35%** | **−40%** | 12 of 13 |
| solx 0.2 preview | **−1.7%** | **−42%** | **−48%** | 10 of 13 |
| oksolc 0.2.2 | **−2.7%** | **−29%** | **−33%** | 11 of 11 |

These are whole-transaction numbers, including the fixed 21,000 gas every transaction pays, so the savings on the code itself are larger. On code with more computation the gap widens. A few examples:

- **Loops over calldata arrays** (sums, maxima, searches): **48% less gas** than solc, 17% less than solx.
- **OpenZeppelin's P256 signature verification**: **15% less gas** than solc.
- **Solady's LibString**: **13.5% less gas** than solc.
- **Tempo Zones' deposit portal**: **28% cheaper to deploy**, and 6.8 KB smaller.

Here is the first example from [solx's launch post](https://paragraph.com/@zksync/solx-the-new-solidity-compiler-why-it-matters-to-you), a factorial that writes to storage on every iteration:

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

solx needed `unchecked` to move the storage write out of the loop. Solar does not: the loop runs in registers with full overflow checks, and the function ends with a single `SSTORE`, since a revert would undo any earlier write anyway. Across our six benchmark calls to this contract, Solar's code uses 186,088 gas, against 209,744 for solx and 224,168 for solc.

## With Solar, safe Solidity beats hand-written assembly.

Today, the cheapest Solidity is written in inline assembly with `unchecked` math. Libraries like [Solady](https://github.com/Vectorized/solady) are brilliant, but every line of assembly skips the compiler's checks, and a single mistake can cost users their funds. We think the compiler, not the developer, should do that work.

To test this, we rewrote all 90 sources of Solady v0.1.26 in plain, checked Solidity ([safe Solady](https://github.com/djolertrk/solady/tree/feat/safe-solady-natspec) [move to paradigmxyz before publishing]). Assembly and `unchecked` appear only in small functions tagged for review, about one in ten, which do what Solidity cannot express, such as reading raw storage slots or bubbling up a revert. The rewrite keeps every declaration and passes Solady's own tests under both solc and Solar.

We then ran Solady's own test suites against both versions on the same inputs:

- The safe rewrite compiled with Solar is **cheaper in 1,479 of 1,558 tests (95%)**.
- On average it uses **19% less gas** than the original assembly compiled with solc.
- On a separate benchmark of 28,413 calls across ten libraries, checked against an independent model of each function, it uses **41% less gas** than the cheaper of solc's two pipelines for each call, and is cheaper on 9 calls out of 10.

In other words: write readable, checked code, and let the compiler make it fast.

*[Editorial: this section depends on [paradigmxyz/solar#1999](https://github.com/paradigmxyz/solar/pull/1999) landing before the release.]*

## How does Solar compare to other compilers?

We are glad to see more teams working on Solidity compilers. Here is how they compare:

- **solc** is the reference compiler. Solar matches its language and output formats, and beats it on compile time, gas and size on every project in our suite.
- **solx**, the LLVM-based compiler now developed by the Nomic Foundation (the Hardhat team), focuses on runtime gas. Against solx 0.1.8, Solar compiles 5× faster on single contracts and 19× faster on whole projects, uses 1.4% less gas, and emits 40% smaller code. solx wins one of our 13 workloads (P256, by 2.7%).
- **solx 0.2**, Nomic's upcoming rewrite on the Slang frontend and MLIR, is faster than 0.1.8 but still 13× slower than Solar on whole projects. On gas, its October preview wins 3 of 13 workloads, each by under 1%, while its code is almost twice the size of Solar's on average, and it still cannot build Seaport, forge-std or PRBMath.
- **oksolc** is a port of solc to Zig. It emits the same bytecode as solc, so Solar's gas and size wins carry over; Solar compiles 4× faster on single contracts and 18× faster on whole projects.

## Why Solar is fast and its code is good.

Solar is built like a modern compiler, in the style of rustc and LLVM:

```
Solidity → AST → HIR (types) → MIR → EVM IR → bytecode
```

- **MIR** is a typed SSA representation of your contract. It still knows what a mapping, an array or a checked addition is, which lets it remove redundant checks, keep storage values in registers across loops, inline, and fold constants with full knowledge of Solidity's semantics.
- **EVM IR** is close to the machine: basic blocks of opcodes with explicit stack operations. It handles stack scheduling, block layout, tail merging and peephole rewrites, all priced by one gas and size cost model.

Each layer does one job, so each optimization lives in one place and is easy to test, and new targets or analyses plug in at the right level. Solar's frontend already powers `forge lint` and Foundry's language server; the new layers are what make the next steps below possible.

## Solar will be Foundry's default compiler.

We intend to make Solar the default compiler in Foundry, so every `forge build` and `forge test` gets these wins with no changes to your code.

Correctness comes first. Every change to Solar runs thousands of tests and a differential benchmark against solc, and we run the full test suites of Morpho Blue, Solmate, Solady, Seaport, OpenZeppelin and Uniswap v4 under both compilers: 3,189 tests, with Solar matching solc on every project [re-verify on the release commit].

You can try it today in any Foundry project:

```bash
# [install command TBD, e.g. solarup or cargo install]
SOLC_WRAPPER=1 FOUNDRY_SOLC=$(which solar) forge test
```

If your project does not build, or a test behaves differently from solc, please [open an issue](https://github.com/paradigmxyz/solar/issues). That is the most valuable feedback you can give us right now.

## Where do we go from here?

Since we announced Solar, we have:

1. [Introduced Solar](https://www.paradigm.xyz/writing/solar) as a fast Solidity frontend.
2. Shipped Solar inside Foundry, powering `forge lint` and `forge lsp` [confirm].
3. Built the MIR and EVM IR middle and back end.
4. Released full code generation, faster and cheaper than solc (this post).

Next, we are working on:

- **Security from the compiler.** Our IRs carry far more information than bytecode. We will use them to check that each optimization preserves behavior, verify rewrite rules formally (some already are, in Lean), and flag unsafe patterns in your code before they ship. Safe Solady is the first step: Solar can already report which contracts contain no assembly or `unchecked` code outside reviewed functions.
- **A Solar standard library**, so safe primitives like the ones in safe Solady need no assembly at all.
- **Debug info** through [ethdebug](https://github.com/ethdebug/format), so optimized contracts stay easy to debug in Foundry.
- **LLM-assisted optimizations**, where models propose transformations and the compiler proves them correct before applying them.

If you are a compiler engineer and want to work on this, reach out to [georgios@paradigm.xyz](mailto:georgios@paradigm.xyz). And if you write Solidity, [go try Solar](https://github.com/paradigmxyz/solar) on your project and tell us what you find.

See you on [GitHub](https://github.com/paradigmxyz/solar).
