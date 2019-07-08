# mpir

This is an old version of mpir. It no longer works with the latest version of SealPIR. It is here for purposes of reproducing 
the results of our [paper](https://eprint.iacr.org/2017/1142.pdf).

What you need: 

(1) SEAL 2.3.1. This is an older version of SEAL and is no longer available on the SEAL website, but you can find it online and in github 
(e.g., https://github.com/barlettacarmen/CrCNN/tree/master/SEAL_2.3.1).

(2) SealPIR-Rust (https://github.com/sga001/sealpir-rust).


# Building

- First install SEAL 2.3.1 following the directions in that repo (see INSTALL.txt).

- Compile SealPIR-Rust and test it to make sure it works. Make sure you are using Rust 1.30 or 1.31 nightly. You can use
rustup: ``$ rustup override set nightly-2018-10-24``

You can then compile and test SealPIR-Rust with cargo: ``$ cargo test``
 
- Modify the Cargo.toml file in this repo (mpir) to specify the path of SealPIR-rust (right now it assumes it is located 
at ``../sealpir-rust/``).

- Compile mpir with cargo and test that it works: ``$ cargo test``

# Reproducing results

Run ``cargo bench`` to reproduce the experiments in the paper.
