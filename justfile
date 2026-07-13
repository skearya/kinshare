set dotenv-required

BIN := "client"

default: build copy run

build:
    RUSTFLAGS="-C target-feature=+crt-static -C opt-level=3 -C strip=symbols" cross build --target armv7-unknown-linux-musleabihf --bin {{ BIN }} --release

copy:
    SSHPASS=$SSHPASS sshpass -e scp target/armv7-unknown-linux-musleabihf/release/{{ BIN }} $HOST:/mnt/us/dev/{{ BIN }}

copy-key:
    SSHPASS=$SSHPASS sshpass -e scp server.key $HOST:/mnt/us/dev/server.key

run:
    SSHPASS=$SSHPASS sshpass -e ssh $HOST "/mnt/us/dev/{{ BIN }}"

convert:
    magick -size 1872x2480 -depth 8 gray:raw/frame.raw out/frame.png
