# rusty-git

A minimal Rust implementation of `git`, following the
[codecrafters.io](https://codecrafters.io/) tutorial
found [here](https://github.com/codecrafters-io/build-your-own-git/blob/main/course-definition.yml).

![term-recording.gif](./assets/rusty-git.gif)

## Usage

The following git commands are (at least partially) implemented:

- [x] `cat-file`
- [x] `checkout`
- [x] `clone`
- [x] `commit-tree`
- [x] `hash-object`
- [x] `index-pack`
- [x] `init`
- [x] `ls-tree`
- [ ] `unpack-objects`
- [ ] `verify-pack`
- [x] `write-tree`

Note that some optional flags aren't supported; git's staging area is also not
implemented, nor is support for the `.gitignore` file.

### TODO

- use non-blocking HTTP calls to get real-time server responses
- implement staging area, so `checkout` shows clean working directory

Here are some things you can do:

### Initialize a new repository

Initialize a git repository in the current working directory.

```
$ rusty-git init
```

### Clone an existing repository

Clone a repository from an HTTPS remote (SSH not supported).

```
$ rusty-git clone https://github.com/superhawk610/rusty-git
```

### Compute the object hash for a file

Determine the SHA1 hash for the given file (where it would be stored in the
`.git/objects` directory).

```
$ rusty-git hash-object -w hello.txt
```

### Display the contents of a blob

Pretty-print the contents of the git blob at the given hash.

```
$ rusty-git cat-file -p 0d1531376bf63c59c5d81b25598022302770569f
```

### Index a packfile

Given a `.pack` packfile containing one or more objects, generate an `.idx`
index file mapping its contents.

```
$ rusty-git index-pack repo.pack
```

### Verify a packfile (partial support)

Given an `.idx` index file, verify that the corresponding `.pack` packfile
exists and matches the described contents from the index.

```
$ rusty-git verify-pack repo.idx
```

## Building

To build, you'll need a [Rust toolchain](https://rustup.rs/) installed. Then,
you can run

```
$ cargo build --release
$ ./target/release/rusty-git --help
```

During development, you can build and run in a single command with `cargo run`

```
$ cargo run -q -- init
```

## License

This repository is made available under the MIT license.

&copy; 2024 Aaron Ross. All rights reserved.
