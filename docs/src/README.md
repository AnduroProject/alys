# Introduction

This documentation describes:

- The Optimistic Merged Mining protocol for block production by federation members through PoA and finalization through merged mining by Bitcoin miners and approval by the federation.
- The two-way peg between Bitcoin and the Alice sidechain through the federation members.

The documentation intends to provide a high-level overview of the protocol and to provide a reference for implementors.

## Goals

- **Smart contracts and equivalence with the Ethereum Virtual Machine (EVM).** The Alice sidechain will enable the creation and execution of EVM smart contracts, primarily developed using the Solidity programming language. The EVM version deployed on Alice will be equivalent to that of Ethereum, ensuring compatibility with existing developer tooling (Hardhat, Foundry, Remix, …), wallets (Metamask, WalletConnect 2.0 supported wallets,...), as well as key infrastructure, including but not limited to block explorers and data analytics.
- **Merged-mining with Bitcoin**, i.e., Alice will re-use Bitcoin’s Proof-of-Work security as a means to achieve consensus on processed transactions and pay fees to Bitcoin miners in return for this service. This means participating Bitcoin miners will (i) collect transactions processed on Alice by running full nodes or relying on professional transaction processing services (Sequencers) and (ii) include commitments to Alice blocks into Bitcoin block templates used as input to Bitcoin’s PoW. While Alice will have significantly faster block times (e.g. 2 seconds) than Bitcoin, Alice nodes will only consider Alice blocks final once a Bitcoin block (or candidate with sufficient difficulty) commits to the latest state of Alice. A simplified visualization is provided in Figure 1 below.
- **Transaction sequencing via a federation.** Collections of transactions and construction of blocks on Alice will be handled by a federation of pre-defined signers (the “Federation”). The Federation will collaboratively process transactions using a Round-robin or similar protocol that ensures high throughput and will charge a processing fee. Changes to the federation will require a consensus majority among existing signers.
- **Federated BTC bridge (two-way peg).** Alice will integrate a federated BTC bridge, operated by the signers of the Federation, i.e., the Federation will maintain control custody of the BTC locked on Bitcoin while a wrapped version of the BTC exists on Alice. Spending the locked BTC on Bitcoin requires majority consensus among signers.
- **BTC as fee currency.** Users of Alice will be able to pay for transaction fees in BTC (specifically, the federated wrapped BTC version), i.e., users will not have to acquire any 3rd party assets to interact with the network.
- **Governance via a federation.** Changes to the protocol will require a majority consensus among members of the Federation.

## Contribute

This documentation is open source and maintained by the community. If you find any errors or want to suggest improvements, please open an issue or submit a pull request.

You can run a local version of the book.

Install [mdBook](https://rust-lang.github.io/mdBook/index.html):

```bash
cargo install mdbook
```

Make sure you are in the `docs` directory:

```bash
cd docs
```

Run the book:

```bash
mdbook serve
```
