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
# Intel NEO is not packaged in Debian, so it is fetched as pinned upstream
# .debs (Tiger Lake is a Production target in this release); Mesa Rusticl
# stays installed as the fallback ICD. The miner reaches OpenCL via dlopen
# with no link-time deps.
ARG NEO_VER=26.35.39758.10
ARG NEO_GMMLIB=22.10.0
ARG IGC_TAG=v2.41.5
ARG IGC_BUILD=2.41.5+22716
RUN printf 'Types: deb\nURIs: http://deb.debian.org/debian\nSuites: trixie-backports\nComponents: main contrib non-free\nSigned-By: /usr/share/keyrings/debian-archive-keyring.pgp\n' \
        > /etc/apt/sources.list.d/backports.sources \
    && apt-get update && apt-get install -y --no-install-recommends -t trixie-backports \
        libegl-mesa0 libgl1-mesa-dri mesa-opencl-icd \
    && apt-get install -y --no-install-recommends \
        libegl1 libgles2 \
        libcjson1 libdrm2 libdrm-intel1 libgbm1 \
        ocl-icd-libopencl1 libstdc++6 \
        ca-certificates curl \
    && mkdir -p /tmp/neo && cd /tmp/neo \
    && curl -fsSLO "https://github.com/intel/intel-graphics-compiler/releases/download/${IGC_TAG}/intel-igc-core-2_${IGC_BUILD}_amd64.deb" \
    && curl -fsSLO "https://github.com/intel/intel-graphics-compiler/releases/download/${IGC_TAG}/intel-igc-opencl-2_${IGC_BUILD}_amd64.deb" \
    && curl -fsSLO "https://github.com/intel/compute-runtime/releases/download/${NEO_VER}/intel-ocloc_${NEO_VER}-0_amd64.deb" \
    && curl -fsSLO "https://github.com/intel/compute-runtime/releases/download/${NEO_VER}/intel-opencl-icd_${NEO_VER}-0_amd64.deb" \
    && curl -fsSLO "https://github.com/intel/compute-runtime/releases/download/${NEO_VER}/libigdgmm12_${NEO_GMMLIB}_amd64.deb" \
    && printf '%s  %s\n' \
        "c64bff586edf2bd9b49f3e4c9c2be25e05c43466e4dafa2a2c3948d498c736b7" "intel-ocloc_${NEO_VER}-0_amd64.deb" \
        "61712caaddeba3d38e4f79e2a0fb23fea25596ca2d72c3144c6eea2331ec4301" "intel-opencl-icd_${NEO_VER}-0_amd64.deb" \
        "6031a63d6e8a12ce61c14efc15f2c8e727061286e3820b8594e6d00615e04d54" "libigdgmm12_${NEO_GMMLIB}_amd64.deb" \
        | sha256sum -c - \
    && dpkg -i /tmp/neo/*.deb \
    && apt-get install -y -f --no-install-recommends \
    && apt-get purge -y --auto-remove curl \
    && rm -rf /tmp/neo /var/lib/apt/lists/* \
 && groupadd -g 992 render \
 && useradd -m -u 1000 miner \
 && usermod -aG video,render miner
COPY --from=build /src/miner /usr/local/bin/miner
USER miner
WORKDIR /home/miner
ENTRYPOINT ["/usr/local/bin/miner"]
