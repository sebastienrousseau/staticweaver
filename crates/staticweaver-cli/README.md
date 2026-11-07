# staticweaver-cli

The `staticweaver` command-line tool, built on the
[staticweaver](https://crates.io/crates/staticweaver) templating engine.

```sh
cargo install staticweaver-cli
echo 'Hi {{name}}!' | staticweaver render - --set name=Ada
```

Run `staticweaver --help` for every option.

Licensed under MIT or Apache-2.0, at your option.
