FROM stellar/quickstart@sha256:db4515ce17553e635af0866b1408e052beb22a9f2b0e9ed463f87dbcdbe3806a AS build
USER root
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install --no-install-recommends -y \
    build-essential gcc-14 g++-14 pkg-config autoconf automake libtool \
    bison flex libpq-dev curl ca-certificates git parallel perl python3
WORKDIR /build/stellar-core
COPY stellar-core/ ./
ENV CARGO_HOME=/build/cargo RUSTUP_HOME=/build/rustup
ENV PATH=/build/cargo/bin:$PATH
RUN ./install-rust.sh
ENV CC=gcc-14 CXX=g++-14 CARGO_BUILD_JOBS=2
ENV CFLAGS="-O2 -g0" CXXFLAGS="-O2 -g0"
RUN ./autogen.sh && ./configure --disable-tests
# Keep each Soroban protocol's pinned dependency graph. Disable compiler LTO and
# debug information to fit the isolated fixture build within a 4 GiB daemon.
ENV CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_DEBUG=0
ENV CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 CARGO_BUILD_JOBS=1
RUN make -j2 -C lib
RUN --mount=type=cache,id=fuul-core-archive-20260919-cargo-registry,target=/build/cargo/registry \
    --mount=type=cache,id=fuul-core-archive-20260919-cargo-git,target=/build/cargo/git \
    --mount=type=cache,id=fuul-core-archive-20260919-target,target=/build/stellar-core/target \
    --mount=type=cache,id=fuul-core-archive-20260919-p21,target=/build/stellar-core/src/rust/soroban/p21/target \
    --mount=type=cache,id=fuul-core-archive-20260919-p22,target=/build/stellar-core/src/rust/soroban/p22/target \
    --mount=type=cache,id=fuul-core-archive-20260919-p23,target=/build/stellar-core/src/rust/soroban/p23/target \
    --mount=type=cache,id=fuul-core-archive-20260919-p24,target=/build/stellar-core/src/rust/soroban/p24/target \
    --mount=type=cache,id=fuul-core-archive-20260919-p25,target=/build/stellar-core/src/rust/soroban/p25/target \
    --mount=type=cache,id=fuul-core-archive-20260919-p26,target=/build/stellar-core/src/rust/soroban/p26/target \
    --mount=type=cache,id=fuul-core-archive-20260919-p27,target=/build/stellar-core/src/rust/soroban/p27/target \
    --mount=type=cache,id=fuul-core-archive-20260919-p28,target=/build/stellar-core/src/rust/soroban/p28/target \
    make -j2 LDFLAGS=-Wl,--no-keep-memory
RUN mkdir -p /build/evidence && \
    src/stellar-core version > /build/evidence/core-version.json && \
    sha256sum src/stellar-core > /build/evidence/core-binary.sha256 && \
    dpkg-query -W > /build/evidence/build-packages.txt && \
    rustc --version > /build/evidence/rustc-version.txt && \
    g++-14 --version > /build/evidence/compiler-version.txt

FROM stellar/quickstart@sha256:db4515ce17553e635af0866b1408e052beb22a9f2b0e9ed463f87dbcdbe3806a
LABEL org.opencontainers.image.title="Fuul isolated archival test fixture"
LABEL org.opencontainers.image.description="TEST ONLY: Core v28.0.1 with minimum maxEntryTTL validation set to 128"
LABEL org.opencontainers.image.revision="947aad8413c189d85504acf72207e85eeda9b021"
COPY --from=build /build/stellar-core/src/stellar-core /usr/bin/stellar-core
COPY --from=build /build/evidence /usr/share/fuul-archival-build
COPY test-only-ttl.patch build-provenance.json /usr/share/fuul-archival-build/
RUN /usr/bin/stellar-core version
