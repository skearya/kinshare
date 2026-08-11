set dotenv-load

BIN := "kinshare-client"

default: build copy

build:
    RUSTFLAGS="-C target-feature=+crt-static -C opt-level=3 -C strip=symbols" cargo zigbuild --target armv7-unknown-linux-musleabihf --bin {{ BIN }} --release

copy:
    SSHPASS=$SSHPASS sshpass -e scp -r extension $HOST:/mnt/us/extensions/kinshare
    SSHPASS=$SSHPASS sshpass -e scp target/armv7-unknown-linux-musleabihf/release/{{ BIN }} $HOST:/mnt/us/extensions/kinshare/bin/{{ BIN }}

copy-keys:
    SSHPASS=$SSHPASS sshpass -e scp connection.keys $HOST:/mnt/us/extensions/kinshare/connection.keys

convert:
    magick -size 1872x2480 -depth 8 gray:raw/frame.raw out/frame.png
