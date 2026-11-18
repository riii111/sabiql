# Recording the README demo

On macOS, install VHS, ttyd, FFmpeg, PostgreSQL (client and server tools), Graphviz, Chrome, Python with Pillow, and DroidSansM Nerd Font Mono. Run these commands from the repository root with a local wallpaper image and an empty temporary directory.

```sh
cargo build --locked
export SABIQL_DEMO_BIN="$PWD/target/debug/sabiql"
export SABIQL_DEMO_DIR=$(mktemp -d /tmp/sabiql-demo.XXXXXX)
export VHS_CHROME_PATH="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
bash assets/demo/tour/capture.sh
python3 assets/demo/tour/compose.py "$SABIQL_DEMO_DIR" /path/to/wallpaper.png assets/demo/tour/shop-rhythm.gif
```

The capture starts a temporary PostgreSQL server on a private Unix socket. The recording, database, and generated ER diagram stay in the temporary directory for inspection.
