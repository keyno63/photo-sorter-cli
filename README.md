# photo-sorter-cli

CLI to sort photos by EXIF capture date (`yyyymmdd`) into another directory.
The default mode is `dry-run`. Actual move runs only with `--execute`.

## Behavior

- Recursively scans `--source`
- Reads EXIF date (`DateTimeOriginal`, fallback to other EXIF date tags)
- Moves image files into `--destination/<yyyymmdd>/`
- Files with no EXIF date, and non-image files, go to `--destination/その他/`
- In dry-run mode, writes a tree-style report to `selected_picture_result.txt` (or `--report-file`)

## Usage

```bash
# dry-run (default)
cargo run -- --source "C:\\photos\\input" --destination "D:\\sorted"
```

```bash
# execute real move
cargo run -- --source "C:\\photos\\input" --destination "D:\\sorted" --execute
```

```bash
# custom report path
cargo run -- --source "C:\\photos\\input" --destination "D:\\sorted" --report-file ".\\my_result.txt"
```

## Build binary

```bash
cargo build --release
```

Output binary:

- `target/release/photo-sorter-cli.exe`
