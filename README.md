# Fuzio Games Contracts

Repository that will contain all Fuzio games

## Contracts

| Name                                   | Description             |
| -------------------------------------- | ----------------------- |
| [`fuzio_option_trading`](contracts/fuzio_option_trading)   | Prediction contract |

## Building Contracts

You will need Rust 1.64.0+ with wasm32-unknown-unknown target installed.

### You can compile each contract:

Go to contract directory and run

```
cargo wasm
```

### For a production-ready (compressed) build:

Run the following from the repository root

```
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/rust-optimizer:0.12.13
```

The optimized contracts are generated in the artifacts/ directory.

## Docs

Docs can be generated using `cargo doc --no-deps`
