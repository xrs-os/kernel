FROM rust:1-buster as kernel

ENV CARGO_HOME=/usr/local/cargo \
    RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static \
    RUSTUP_UPDATE_ROOT=https://mirrors.ustc.edu.cn/rust-static/rustup

RUN cargo install cargo-binutils

WORKDIR /xrs-os
COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/xrs-os/target \
    python3 bootstrap.py --release build initfs.img

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/xrs-os/target \
    python3 bootstrap.py --release build kernel.bin


FROM debian as opensbi

RUN sed -i 's/deb.debian.org/mirrors.ustc.edu.cn/g' /etc/apt/sources.list \
    && apt-get update -y && apt-get install -y git autoconf automake \
    autotools-dev curl python3 libmpc-dev \
    libmpfr-dev libgmp-dev gawk build-essential \
    bison flex texinfo gperf libtool patchutils \
    bc zlib1g-dev libexpat-dev

WORKDIR /build

# build riscv-gnu-toolchain
RUN git clone --depth 1 --branch 2022.04.23 https://github.com/riscv-collab/riscv-gnu-toolchain.git

RUN cd riscv-gnu-toolchain && ./configure --prefix=/opt/riscv && make -j$(nproc)

ENV PATH="/opt/riscv/bin:${PATH}"

COPY --from=kernel /xrs-os/build/kernel.bin .

# build opensbi
RUN git clone --depth 1 --branch v1.0 https://github.com/riscv-software-src/opensbi.git

RUN cd opensbi && make CROSS_COMPILE=riscv64-unknown-elf- PLATFORM=generic \
    FW_PAYLOAD_PATH=../kernel.bin O=../dist


FROM scratch AS dist

WORKDIR /dist
COPY --from=kernel /xrs-os/build/initfs.img .
COPY --from=opensbi /build/dist/platform/generic/firmware/fw_dynamic.elf .
