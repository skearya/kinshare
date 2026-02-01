set dotenv-required

BIN := "client"

default: build copy run

build:
    RUSTFLAGS="-C target-feature=+crt-static -C opt-level=3 -C strip=symbols -C target-feature=+v7,+neon,+aes" cross build --target arm-unknown-linux-musleabi --bin {{BIN}} --release

copy:
    SSHPASS=$SSHPASS sshpass -e scp target/arm-unknown-linux-musleabi/release/{{BIN}} $HOST:/mnt/us/dev/{{BIN}}

run:
    SSHPASS=$SSHPASS sshpass -e ssh $HOST "/mnt/us/dev/{{BIN}}"

convert:
    magick -size 1872x2480 -depth 8 gray:raw/frame.raw out/frame.png
