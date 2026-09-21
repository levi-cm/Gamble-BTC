FROM debian:trixie-slim AS build
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential pkg-config ca-certificates \
        libegl-dev libgles-dev libgl-dev libcjson-dev \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Makefile /src/
COPY src/ /src/src/
RUN make \
 && strip miner

FROM debian:trixie-slim
ENV DEBIAN_FRONTEND=noninteractive
# Mesa 26.x (backports) for the Iris Xe compiler path plus OpenCL ICD support.
# Intel NEO is not packaged in Debian, so OpenCL comes from Mesa Rusticl
# (mesa-opencl-icd), which the miner reaches via dlopen with no link-time deps.
RUN printf 'Types: deb\nURIs: http://deb.debian.org/debian\nSuites: trixie-backports\nComponents: main contrib non-free\nSigned-By: /usr/share/keyrings/debian-archive-keyring.pgp\n' \
        > /etc/apt/sources.list.d/backports.sources \
    && apt-get update && apt-get install -y --no-install-recommends -t trixie-backports \
        libegl-mesa0 libgl1-mesa-dri mesa-opencl-icd \
    && apt-get install -y --no-install-recommends \
        libegl1 libgles2 \
        libcjson1 libdrm2 libdrm-intel1 libgbm1 \
        ocl-icd-libopencl1 \
        ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd -g 992 render \
 && useradd -m -u 1000 miner \
 && usermod -aG video,render miner
COPY --from=build /src/miner /usr/local/bin/miner
USER miner
WORKDIR /home/miner
ENTRYPOINT ["/usr/local/bin/miner"]
