# Fast katcp encoding and decoding

This library is implemented in Rust to provide more efficient encoding and
decoding of
[katcp](https://katcp-python.readthedocs.io/en/latest/_downloads/361189acb383a294be20d6c10c257cb4/NRF-KAT7-6.0-IFCE-002-Rev5-1.pdf)
messages than a pure-Python library is likely to be able to
do. It is not currently intended to be consumed by other Rust libraries, but
the API is structured so as to allow this. If you find yourself with a need to
call this code from Rust, please file to ticket so that I can investigate
publishing a crate independently of the Python package.

## Installation

Run `pip install katcp_codec`.

The package is published with binary wheels for CPython 3.8-3.12 on Linux
and MacOS (x86-64 and AArch 64), which means you will not need a Rust compiler
for those platforms. For other platforms, you will need the Rust compiler.
You can find simple installation instructions at
[rustup.rs](https://rustup.rs).

## Usage

Refer to the [online manual](https://katcp-codec.readthedocs.io/en/latest).
