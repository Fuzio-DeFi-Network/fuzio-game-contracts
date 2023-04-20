# Fuzio Option Trading

Fuzio option trading game with fuzion cw20 token on sei network.

## General Contracts

| Name                                   | Description             |
| -------------------------------------- | ----------------------- |
| [`fuzio_option_trading`](contracts/fuzio_option_trading)   | Option Trading contract |
| [`fast-oracle`](contracts/fast-oracle)                     | To set the price of sei |

## Building Contracts

You will need Rust 1.64.0+ with wasm32-unknown-unknown target installed.

### You can compile each contract:

Go to contract directory and run

```
cargo wasm
cp ../../target/wasm32-unknown-unknown/release/archon_token.wasm .
ls -l archon_token.wasm
sha256sum archon_token.wasm
```

### You can run tests for all contracts

Run the following from the repository root

```
cargo test
```

### For a production-ready (compressed) build:

Run the following from the repository root

```
./scripts/build_release.sh
```

The optimized contracts are generated in the artifacts/ directory.

## Docs

Docs can be generated using `cargo doc --no-deps`
