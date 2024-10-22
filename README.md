# hdl

Manga downloader

## Supported links

- `https://imhentai.xxx/gallery/123..../`
- `https://nhentai.net/g/123..../`
- `https://e-hentai.org/g/1234567/12345abcdef/`

## ~~quick~~ blazingly fast start

```sh
cargo build --release

./target/release/hdl -h
```

```
Usage: hdl [OPTIONS] <URL>

Arguments:
  <URL>

Options:
  -j, --jobs <JOBS>        parallel jobs count [default: 1]
  -o, --out-dir <OUT_DIR>  [default: ./out/]
  -h, --help               Print help

```
