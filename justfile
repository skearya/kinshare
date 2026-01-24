set dotenv-required

default: build copy run

build:
    RUSTFLAGS="-C target-feature=+crt-static -C opt-level=s -C strip=symbols" cross build --target arm-unknown-linux-musleabi --bin client --release

copy:
    SSHPASS=$SSHPASS sshpass -e scp target/arm-unknown-linux-musleabi/release/client $HOST:/mnt/us/dev/client

run:
    SSHPASS=$SSHPASS sshpass -e ssh $HOST "/mnt/us/dev/client"

convert:
    magick -size 1872x2480 -depth 8 gray:raw/frame.raw out/frame.png
